//! The append-only event log (ARCHITECTURE.md 8.1).

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

use crate::clock::{Clock, format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::AppError;
use crate::page::{Page, PageRequest};

/// An event about to be appended. The service builds it from its clock and
/// the entity view, so `append_event` needs no clock of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct NewEvent {
    pub id: Uuid,
    pub occurred_at: DateTime<Utc>,
    /// `<entity>.<verb>`, for example `transaction.created`.
    pub event_type: String,
    pub entity_type: String,
    pub entity_id: String,
    /// The entity as the API returns it; for `deleted`, its last state.
    pub data: Value,
}

impl NewEvent {
    pub fn new<T: Serialize>(
        clock: &dyn Clock,
        entity_type: &str,
        verb: &str,
        entity_id: String,
        data: &T,
    ) -> Result<Self, AppError> {
        let data = serde_json::to_value(data)
            .map_err(|e| AppError::Internal(format!("cannot serialise event data: {e}")))?;
        Ok(Self {
            id: new_id(clock),
            occurred_at: clock.now(),
            event_type: format!("{entity_type}.{verb}"),
            entity_type: entity_type.to_string(),
            entity_id,
            data,
        })
    }
}

/// Appends one event inside the caller's transaction and returns its `seq`.
///
/// Pass `&mut tx` for an open `Transaction<'_, Sqlite>`: a transaction
/// dereferences to its connection. The transaction's `'_` lifetime says it
/// borrows a pooled connection and cannot outlive it, like a Go `*sql.Tx` you
/// must not use after the connection closes, but enforced by the compiler.
/// If the insert fails, the caller's `?` returns before `commit`, and dropping
/// an uncommitted transaction rolls the whole change back.
pub async fn append_event(
    tx: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    origin: Origin,
    event: NewEvent,
) -> Result<i64, AppError> {
    let id = event.id.to_string();
    let household_id = ctx.household_id.to_string();
    let actor = ctx.user_id.to_string();
    let origin = origin.as_str();
    let occurred_at = format_timestamp(event.occurred_at);
    let payload = json!({
        "id": id,
        "type": event.event_type,
        "occurred_at": occurred_at,
        "household_id": household_id,
        "actor": { "user_id": actor },
        "origin": origin,
        "data": event.data,
    })
    .to_string();

    // `query!` checks this SQL against the real schema at compile time, like
    // `sqlc` in Go or PHPStan understanding your SQL.
    let seq = sqlx::query_scalar!(
        "INSERT INTO events (id, household_id, event_type, entity_type, entity_id,
                             actor_user_id, origin, payload, occurred_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         RETURNING seq AS \"seq!\"",
        id,
        household_id,
        event.event_type,
        event.entity_type,
        event.entity_id,
        actor,
        origin,
        payload,
        occurred_at,
    )
    .fetch_one(&mut *tx)
    .await?;
    Ok(seq)
}

/// One row of the event log, as `GET /events` returns it.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct EventView {
    pub seq: i64,
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub entity_type: String,
    pub entity_id: String,
    pub actor_user_id: Option<String>,
    pub origin: String,
    pub occurred_at: String,
    /// The full payload, also the webhook body.
    #[schema(value_type = Object)]
    pub payload: Value,
}

/// Filters for listing events.
#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    /// Only events with a larger `seq`.
    pub after_seq: Option<i64>,
    /// Only this type, for example `transaction.created`.
    pub event_type: Option<String>,
}

/// Reads the event log.
#[derive(Clone)]
pub struct EventService {
    pool: SqlitePool,
}

impl EventService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Events of the caller's household in `seq` order.
    pub async fn list(
        &self,
        ctx: &HouseholdCtx,
        filter: &EventFilter,
        page: PageRequest,
    ) -> Result<Page<EventView>, AppError> {
        let household_id = ctx.household_id.to_string();
        let after = filter.after_seq.unwrap_or(0);
        let (limit, offset) = (page.limit(), page.offset());
        let rows = sqlx::query!(
            "SELECT seq, id, event_type, entity_type, entity_id, actor_user_id, origin,
                    payload, occurred_at
             FROM events
             WHERE household_id = ?1 AND seq > ?2 AND (?3 IS NULL OR event_type = ?3)
             ORDER BY seq
             LIMIT ?4 OFFSET ?5",
            household_id,
            after,
            filter.event_type,
            limit,
            offset,
        )
        .fetch_all(&self.pool)
        .await?;
        let total = sqlx::query_scalar!(
            "SELECT count(*) FROM events
             WHERE household_id = ?1 AND seq > ?2 AND (?3 IS NULL OR event_type = ?3)",
            household_id,
            after,
            filter.event_type,
        )
        .fetch_one(&self.pool)
        .await?;

        let items = rows
            .into_iter()
            .map(|row| {
                let payload = serde_json::from_str(&row.payload)
                    .map_err(|e| AppError::Internal(format!("corrupt event payload: {e}")))?;
                Ok(EventView {
                    seq: row.seq,
                    id: row.id,
                    event_type: row.event_type,
                    entity_type: row.entity_type,
                    entity_id: row.entity_id,
                    actor_user_id: row.actor_user_id,
                    origin: row.origin,
                    occurred_at: row.occurred_at,
                    payload,
                })
            })
            // Collecting `Result`s into `Result<Vec<_>, _>` stops at the first error.
            .collect::<Result<Vec<_>, AppError>>()?;
        Ok(Page::new(items, page, total))
    }
}

/// The short form of an event that live listeners (`GET /events/stream`)
/// receive: enough to know what changed, not the whole entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct EventNotice {
    pub seq: i64,
    #[serde(rename = "type")]
    pub event_type: String,
    pub entity_type: String,
    pub entity_id: String,
}

impl EventService {
    /// The household's newest `seq` (0 when it has no events).
    pub async fn latest_seq(&self, ctx: &HouseholdCtx) -> Result<i64, AppError> {
        let household_id = ctx.household_id.to_string();
        let seq = sqlx::query_scalar!(
            r#"SELECT coalesce(max(seq), 0) AS "seq!: i64" FROM events WHERE household_id = ?"#,
            household_id
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(seq)
    }

    /// Up to `limit` notices of the household after `after_seq`, oldest first.
    pub async fn notices_after(
        &self,
        ctx: &HouseholdCtx,
        after_seq: i64,
        limit: i64,
    ) -> Result<Vec<EventNotice>, AppError> {
        let household_id = ctx.household_id.to_string();
        let rows = sqlx::query_as!(
            EventNotice,
            "SELECT seq, event_type, entity_type, entity_id FROM events
             WHERE household_id = ? AND seq > ? ORDER BY seq LIMIT ?",
            household_id,
            after_seq,
            limit,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}
