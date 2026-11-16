//! The dispatcher: turns new events into `webhook_deliveries` rows.
//!
//! It works across households because the `webhooks` cursor is one global
//! position in the event log; each event only ever matches endpoints of its
//! own household (see [`super::matching::endpoint_wants`]).

use sqlx::SqliteConnection;

use super::WebhookService;
use super::matching::{EndpointFilter, EventSummary, endpoint_wants};
use crate::clock::{format_timestamp, new_id};
use crate::error::AppError;

/// The cursor name in `consumer_cursors`.
pub const CURSOR: &str = "webhooks";

/// Events read per transaction.
const BATCH_SIZE: i64 = 200;

/// What one [`WebhookService::dispatch`] call did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DispatchReport {
    pub events: usize,
    pub deliveries: usize,
}

impl WebhookService {
    /// Reads every event after the cursor, in batches. Each batch inserts its
    /// deliveries and moves the cursor in one transaction, so a crash between
    /// batches neither loses nor duplicates a delivery.
    pub async fn dispatch(&self) -> Result<DispatchReport, AppError> {
        let mut report = DispatchReport::default();
        loop {
            let batch = self.dispatch_batch(BATCH_SIZE).await?;
            report.events += batch.events;
            report.deliveries += batch.deliveries;
            if batch.events < usize::try_from(BATCH_SIZE).unwrap_or(usize::MAX) {
                return Ok(report);
            }
        }
    }

    /// One batch of at most `limit` events.
    pub async fn dispatch_batch(&self, limit: i64) -> Result<DispatchReport, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let after = cursor(&mut tx).await?;
        let events = events_after(&mut tx, after, limit).await?;
        let Some(last) = events.last().map(|e| e.seq) else {
            return Ok(DispatchReport::default());
        };
        let endpoints = active_endpoints(&mut tx).await?;
        let now = format_timestamp(self.clock.now());
        let mut deliveries = 0;
        for event in &events {
            for endpoint in endpoints.iter().filter(|e| endpoint_wants(e, event)) {
                let id = new_id(&*self.clock).to_string();
                // OR IGNORE: the UNIQUE (endpoint_id, event_seq) row exists
                // already if this batch ran before (it cannot, since the
                // cursor moves with it, but the guard is free).
                let inserted = sqlx::query!(
                    "INSERT OR IGNORE INTO webhook_deliveries
                         (id, endpoint_id, event_seq, next_attempt_at, created_at)
                     VALUES (?, ?, ?, ?, ?)",
                    id,
                    endpoint.id,
                    event.seq,
                    now,
                    now,
                )
                .execute(&mut *tx)
                .await?;
                deliveries += usize::try_from(inserted.rows_affected()).unwrap_or(0);
            }
        }
        sqlx::query!(
            "INSERT INTO consumer_cursors (consumer, last_seq, updated_at) VALUES (?, ?, ?)
             ON CONFLICT (consumer) DO UPDATE SET last_seq = excluded.last_seq,
                                                 updated_at = excluded.updated_at",
            CURSOR,
            last,
            now,
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(DispatchReport {
            events: events.len(),
            deliveries,
        })
    }
}

async fn cursor(conn: &mut SqliteConnection) -> Result<i64, AppError> {
    let seq = sqlx::query_scalar!(
        "SELECT last_seq FROM consumer_cursors WHERE consumer = ?",
        CURSOR
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(seq.unwrap_or(0))
}

async fn events_after(
    conn: &mut SqliteConnection,
    after: i64,
    limit: i64,
) -> Result<Vec<EventSummary>, AppError> {
    let rows = sqlx::query_as!(
        EventSummary,
        "SELECT seq, household_id, event_type, entity_id, origin, occurred_at
         FROM events WHERE seq > ? ORDER BY seq LIMIT ?",
        after,
        limit,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

async fn active_endpoints(conn: &mut SqliteConnection) -> Result<Vec<EndpointFilter>, AppError> {
    let rows = sqlx::query!(
        "SELECT id, household_id, event_types, include_import_events, created_at
         FROM webhook_endpoints WHERE active = 1"
    )
    .fetch_all(&mut *conn)
    .await?;
    rows.into_iter()
        .map(|row| {
            let event_types = serde_json::from_str(&row.event_types)
                .map_err(|e| AppError::Internal(format!("corrupt event_types: {e}")))?;
            Ok(EndpointFilter {
                id: row.id,
                household_id: row.household_id,
                event_types,
                include_import_events: row.include_import_events != 0,
                created_at: row.created_at,
            })
        })
        .collect()
}
