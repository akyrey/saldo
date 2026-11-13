//! Rows of `webhook_endpoints` and `webhook_deliveries`.

use serde::Serialize;
use sqlx::SqliteConnection;

use super::WebhookView;
use crate::ctx::HouseholdCtx;
use crate::error::AppError;

/// One delivery attempt record, as `GET /webhooks/{id}/deliveries` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct DeliveryView {
    pub id: String,
    pub endpoint_id: String,
    pub event_seq: i64,
    pub event_id: String,
    pub event_type: String,
    /// `pending`, `succeeded` or `dead`.
    pub status: String,
    pub attempts: i64,
    pub next_attempt_at: Option<String>,
    pub last_attempt_at: Option<String>,
    pub last_status_code: Option<i64>,
    pub last_error: Option<String>,
    pub created_at: String,
}

/// The columns of an endpoint row as SQLite returns them.
struct EndpointRow {
    id: String,
    name: String,
    url: String,
    event_types: String,
    include_import_events: i64,
    active: i64,
    consecutive_failures: i64,
    disabled_reason: Option<String>,
    created_at: String,
    updated_at: String,
}

impl EndpointRow {
    fn into_view(self) -> Result<WebhookView, AppError> {
        let event_types = serde_json::from_str(&self.event_types)
            .map_err(|e| AppError::Internal(format!("corrupt event_types: {e}")))?;
        Ok(WebhookView {
            id: self.id,
            name: self.name,
            url: self.url,
            event_types,
            include_import_events: self.include_import_events != 0,
            active: self.active != 0,
            consecutive_failures: self.consecutive_failures,
            disabled_reason: self.disabled_reason,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

/// One endpoint of the caller's household.
pub(super) async fn find(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    id: &str,
) -> Result<WebhookView, AppError> {
    let household_id = ctx.household_id.to_string();
    sqlx::query_as!(
        EndpointRow,
        "SELECT id, name, url, event_types, include_import_events, active,
                consecutive_failures, disabled_reason, created_at, updated_at
         FROM webhook_endpoints WHERE household_id = ? AND id = ?",
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?
    .into_view()
}

/// Every endpoint of the caller's household, by name.
pub(super) async fn list(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<Vec<WebhookView>, AppError> {
    let household_id = ctx.household_id.to_string();
    sqlx::query_as!(
        EndpointRow,
        "SELECT id, name, url, event_types, include_import_events, active,
                consecutive_failures, disabled_reason, created_at, updated_at
         FROM webhook_endpoints WHERE household_id = ? ORDER BY name, id",
        household_id,
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(EndpointRow::into_view)
    .collect()
}

/// Deliveries of one endpoint, newest event first.
pub(super) async fn deliveries(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    endpoint_id: &str,
    limit: i64,
    offset: i64,
) -> Result<(Vec<DeliveryView>, i64), AppError> {
    let household_id = ctx.household_id.to_string();
    let items = sqlx::query_as!(
        DeliveryView,
        r#"SELECT d.id AS "id!", d.endpoint_id, d.event_seq, e.id AS event_id,
                  e.event_type, d.status, d.attempts, d.next_attempt_at, d.last_attempt_at,
                  d.last_status_code, d.last_error, d.created_at
           FROM webhook_deliveries d
           JOIN webhook_endpoints w ON w.id = d.endpoint_id
           JOIN events e ON e.seq = d.event_seq
           WHERE w.household_id = ?1 AND d.endpoint_id = ?2
           ORDER BY d.event_seq DESC LIMIT ?3 OFFSET ?4"#,
        household_id,
        endpoint_id,
        limit,
        offset,
    )
    .fetch_all(&mut *conn)
    .await?;
    let total = sqlx::query_scalar!(
        "SELECT count(*) FROM webhook_deliveries d
         JOIN webhook_endpoints w ON w.id = d.endpoint_id
         WHERE w.household_id = ? AND d.endpoint_id = ?",
        household_id,
        endpoint_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok((items, total))
}

/// One delivery of the caller's household.
pub(super) async fn find_delivery(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    id: &str,
) -> Result<DeliveryView, AppError> {
    let household_id = ctx.household_id.to_string();
    sqlx::query_as!(
        DeliveryView,
        r#"SELECT d.id AS "id!", d.endpoint_id, d.event_seq, e.id AS event_id,
                  e.event_type, d.status, d.attempts, d.next_attempt_at, d.last_attempt_at,
                  d.last_status_code, d.last_error, d.created_at
           FROM webhook_deliveries d
           JOIN webhook_endpoints w ON w.id = d.endpoint_id
           JOIN events e ON e.seq = d.event_seq
           WHERE w.household_id = ? AND d.id = ?"#,
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}
