//! Bookkeeping of import runs (`import_runs`).

use serde_json::Value;
use sqlx::SqlitePool;

use crate::clock::{Clock, format_timestamp, new_id};
use crate::ctx::HouseholdCtx;
use crate::error::AppError;

/// Records the start of a run and returns its ID. Written outside the
/// import's own transaction, so a failed or dry run is recorded too.
pub async fn start(
    pool: &SqlitePool,
    clock: &dyn Clock,
    ctx: &HouseholdCtx,
    source: &str,
) -> Result<String, AppError> {
    let id = new_id(clock).to_string();
    let (household_id, now) = (ctx.household_id.to_string(), format_timestamp(clock.now()));
    sqlx::query!(
        "INSERT INTO import_runs (id, household_id, source, status, started_at) VALUES (?, ?, ?, 'running', ?)",
        id,
        household_id,
        source,
        now,
    )
    .execute(pool)
    .await?;
    Ok(id)
}

/// Marks a run as finished with its summary (counts and messages; never secrets).
pub async fn finish(
    pool: &SqlitePool,
    clock: &dyn Clock,
    id: &str,
    succeeded: bool,
    summary: &Value,
) -> Result<(), AppError> {
    let status = if succeeded { "succeeded" } else { "failed" };
    let (summary, now) = (summary.to_string(), format_timestamp(clock.now()));
    sqlx::query!(
        "UPDATE import_runs SET status = ?, summary = ?, finished_at = ? WHERE id = ?",
        status,
        summary,
        now,
        id,
    )
    .execute(pool)
    .await?;
    Ok(())
}
