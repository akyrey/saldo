//! The delivery worker: POSTs due deliveries, signed, and records the
//! outcome (ARCHITECTURE.md 8.2).

use std::sync::Arc;

use saldo_core::{HouseholdId, UserId};
use sqlx::SqliteConnection;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use super::retry::{MAX_CONSECUTIVE_DEAD, next_delay};
use super::signing::signature_header;
use super::{WebhookService, repo};
use crate::clock::format_timestamp;
use crate::ctx::{HouseholdCtx, Origin, Role};
use crate::error::{AppError, parse_stored};
use crate::events::{NewEvent, append_event};

/// Deliveries picked per round.
const BATCH_SIZE: i64 = 50;
/// Requests in flight at once.
const CONCURRENCY: usize = 4;

pub const USER_AGENT: &str = concat!("Saldo-Webhook/", env!("CARGO_PKG_VERSION"));

/// What one [`WebhookService::deliver_due`] call did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeliveryReport {
    pub succeeded: usize,
    /// Failed, will be retried.
    pub retrying: usize,
    /// Failed for the last time.
    pub dead: usize,
}

/// A pending delivery whose time has come, with everything needed to send it.
#[derive(Debug, Clone)]
struct Due {
    id: String,
    endpoint_id: String,
    household_id: String,
    url: String,
    secret: String,
    event_id: String,
    event_type: String,
    payload: String,
    attempts: i64,
}

/// How a POST went.
#[derive(Debug)]
enum Outcome {
    Success(u16),
    Failure { status: Option<u16>, error: String },
}

impl WebhookService {
    /// Sends every due delivery of an active endpoint, at most
    /// [`CONCURRENCY`] at a time.
    pub async fn deliver_due(&self) -> Result<DeliveryReport, AppError> {
        let client = self.http_client()?;
        let mut report = DeliveryReport::default();
        loop {
            let due = self.due(BATCH_SIZE).await?;
            let full = due.len() == usize::try_from(BATCH_SIZE).unwrap_or(usize::MAX);
            for status in self.send_all(&client, due).await? {
                match status {
                    Sent::Succeeded => report.succeeded += 1,
                    Sent::Retrying => report.retrying += 1,
                    Sent::Dead => report.dead += 1,
                }
            }
            if !full {
                return Ok(report);
            }
        }
    }

    fn http_client(&self) -> Result<reqwest::Client, AppError> {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(self.timeout)
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| AppError::Internal(format!("cannot build the HTTP client: {e}")))
    }

    async fn due(&self, limit: i64) -> Result<Vec<Due>, AppError> {
        let now = format_timestamp(self.clock.now());
        let rows = sqlx::query_as!(
            Due,
            r#"SELECT d.id AS "id!", d.endpoint_id, w.household_id, w.url, w.secret,
                      e.id AS event_id, e.event_type, e.payload, d.attempts
               FROM webhook_deliveries d
               JOIN webhook_endpoints w ON w.id = d.endpoint_id
               JOIN events e ON e.seq = d.event_seq
               WHERE d.status = 'pending' AND d.next_attempt_at <= ? AND w.active = 1
               ORDER BY d.next_attempt_at, d.event_seq LIMIT ?"#,
            now,
            limit,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Sends a batch concurrently and records each outcome.
    ///
    /// A `JoinSet` is a group of spawned tasks you can wait for one by one,
    /// like Go's `errgroup`. The `Semaphore` hands out [`CONCURRENCY`] permits;
    /// a task holds one while it runs, so at most that many requests are open.
    async fn send_all(
        &self,
        client: &reqwest::Client,
        due: Vec<Due>,
    ) -> Result<Vec<Sent>, AppError> {
        let permits = Arc::new(Semaphore::new(CONCURRENCY));
        let mut tasks = JoinSet::new();
        for delivery in due {
            let permit = permits
                .clone()
                .acquire_owned()
                .await
                .map_err(|e| AppError::Internal(format!("semaphore closed: {e}")))?;
            let (service, client) = (self.clone(), client.clone());
            tasks.spawn(async move {
                let outcome = service.post(&client, &delivery).await;
                let result = service.record_outcome(&delivery, outcome).await;
                drop(permit);
                result
            });
        }
        let mut sent = Vec::new();
        while let Some(joined) = tasks.join_next().await {
            let result =
                joined.map_err(|e| AppError::Internal(format!("delivery task failed: {e}")))?;
            sent.push(result?);
        }
        Ok(sent)
    }

    async fn post(&self, client: &reqwest::Client, delivery: &Due) -> Outcome {
        let timestamp = self.clock.now().timestamp();
        let signature = signature_header(&delivery.secret, timestamp, delivery.payload.as_bytes());
        let result = client
            .post(&delivery.url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header("X-Saldo-Event-Id", &delivery.event_id)
            .header("X-Saldo-Event-Type", &delivery.event_type)
            .header("X-Saldo-Delivery-Id", &delivery.id)
            .header("X-Saldo-Signature", signature)
            .body(delivery.payload.clone())
            .send()
            .await;
        match result {
            Ok(response) if response.status().is_success() => {
                Outcome::Success(response.status().as_u16())
            }
            Ok(response) => Outcome::Failure {
                status: Some(response.status().as_u16()),
                error: format!("HTTP {}", response.status()),
            },
            Err(error) => Outcome::Failure {
                status: None,
                error: describe(error, self.timeout),
            },
        }
    }

    /// Stores the outcome; on the last failure also counts the dead delivery
    /// against the endpoint, which may deactivate it.
    async fn record_outcome(&self, delivery: &Due, outcome: Outcome) -> Result<Sent, AppError> {
        let now = self.clock.now();
        let now_text = format_timestamp(now);
        let attempts = delivery.attempts + 1;
        let mut tx = crate::db::begin(&self.pool).await?;
        let sent =
            match outcome {
                Outcome::Success(code) => {
                    let code = i64::from(code);
                    sqlx::query!(
                        "UPDATE webhook_deliveries SET status = 'succeeded', attempts = ?,
                         next_attempt_at = NULL, last_attempt_at = ?, last_status_code = ?,
                         last_error = NULL
                     WHERE id = ?",
                        attempts,
                        now_text,
                        code,
                        delivery.id,
                    )
                    .execute(&mut *tx)
                    .await?;
                    sqlx::query!(
                        "UPDATE webhook_endpoints SET consecutive_failures = 0 WHERE id = ?",
                        delivery.endpoint_id
                    )
                    .execute(&mut *tx)
                    .await?;
                    Sent::Succeeded
                }
                Outcome::Failure { status, error } => {
                    let code = status.map(i64::from);
                    let next =
                        next_delay(attempts, rand::random()).map(|d| format_timestamp(now + d));
                    let state = if next.is_some() { "pending" } else { "dead" };
                    sqlx::query!(
                    "UPDATE webhook_deliveries SET status = ?, attempts = ?, next_attempt_at = ?,
                         last_attempt_at = ?, last_status_code = ?, last_error = ?
                     WHERE id = ?",
                    state, attempts, next, now_text, code, error, delivery.id,
                )
                    .execute(&mut *tx)
                    .await?;
                    if next.is_some() {
                        Sent::Retrying
                    } else {
                        self.count_dead(&mut tx, delivery).await?;
                        Sent::Dead
                    }
                }
            };
        tx.commit().await?;
        Ok(sent)
    }

    /// One more dead delivery in a row; the [`MAX_CONSECUTIVE_DEAD`]th
    /// deactivates the endpoint and appends `webhook.updated`.
    async fn count_dead(
        &self,
        conn: &mut SqliteConnection,
        delivery: &Due,
    ) -> Result<(), AppError> {
        let now = format_timestamp(self.clock.now());
        let reason =
            format!("deactivated after {MAX_CONSECUTIVE_DEAD} consecutive failed deliveries");
        let deactivated = sqlx::query!(
            "UPDATE webhook_endpoints
             SET consecutive_failures = consecutive_failures + 1,
                 active = CASE WHEN consecutive_failures + 1 >= ?1 THEN 0 ELSE active END,
                 disabled_reason = CASE WHEN consecutive_failures + 1 >= ?1 THEN ?2 ELSE disabled_reason END,
                 updated_at = CASE WHEN consecutive_failures + 1 >= ?1 THEN ?3 ELSE updated_at END
             WHERE id = ?4
             RETURNING active",
            MAX_CONSECUTIVE_DEAD,
            reason,
            now,
            delivery.endpoint_id,
        )
        .fetch_one(&mut *conn)
        .await?
        .active
            == 0;
        if deactivated {
            let ctx = owner_ctx(&mut *conn, &delivery.household_id).await?;
            let view = repo::find(&mut *conn, &ctx, &delivery.endpoint_id).await?;
            let event = NewEvent::new(
                &*self.clock,
                "webhook",
                "updated",
                delivery.endpoint_id.clone(),
                &view,
            )?;
            append_event(&mut *conn, &ctx, Origin::System, event).await?;
        }
        Ok(())
    }
}

/// What happened to one delivery in this round.
enum Sent {
    Succeeded,
    Retrying,
    Dead,
}

/// The household's first owner, as the actor of system events.
async fn owner_ctx(
    conn: &mut SqliteConnection,
    household_id: &str,
) -> Result<HouseholdCtx, AppError> {
    let user_id = sqlx::query_scalar!(
        "SELECT user_id FROM household_members WHERE household_id = ? AND role = 'owner'
         ORDER BY created_at, user_id LIMIT 1",
        household_id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(HouseholdCtx {
        household_id: parse_stored::<HouseholdId>(household_id)?,
        user_id: parse_stored::<UserId>(&user_id)?,
        role: Role::Owner,
    })
}

/// A short description of a transport error, without the URL (it may carry
/// a token, as Home Assistant webhook URLs do).
fn describe(error: reqwest::Error, timeout: std::time::Duration) -> String {
    if error.is_timeout() {
        return format!("timed out after {} s", timeout.as_secs());
    }
    if error.is_connect() {
        return "connection failed".to_string();
    }
    let error = error.without_url();
    // Walk the chain of causes, like Go's `errors.Unwrap` in a loop.
    let mut text = error.to_string();
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}
