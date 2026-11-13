//! "Today" in a household's timezone, and the households background tasks
//! work through.

use chrono::NaiveDate;
use chrono_tz::Tz;
use saldo_core::{HouseholdId, UserId};
use sqlx::SqliteConnection;

use super::IdentityService;
use crate::clock::Clock;
use crate::ctx::{HouseholdCtx, Role};
use crate::error::{AppError, parse_stored};

/// Parses an IANA timezone name such as `Europe/Rome`.
pub(crate) fn parse_timezone(field: &str, name: &str) -> Result<Tz, AppError> {
    name.parse::<Tz>().map_err(|_| {
        AppError::invalid(
            field,
            format!("{name:?} is not a known timezone (use a name like Europe/Rome)"),
        )
    })
}

/// The household's calendar date right now. Schedules (bills, recurring
/// transactions, reminders) use this, never the server's local date.
pub async fn today_in(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    clock: &dyn Clock,
) -> Result<NaiveDate, AppError> {
    let id = ctx.household_id.to_string();
    let name = sqlx::query_scalar!("SELECT timezone FROM households WHERE id = ?", id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound)?;
    // A timezone stored before validation existed falls back to UTC.
    let zone = parse_timezone("timezone", &name).unwrap_or(Tz::UTC);
    Ok(clock.now().with_timezone(&zone).date_naive())
}

impl IdentityService {
    /// [`today_in`] on a pooled connection.
    pub async fn household_today(&self, ctx: &HouseholdCtx) -> Result<NaiveDate, AppError> {
        let mut conn = self.pool.acquire().await?;
        today_in(&mut conn, ctx, &*self.clock).await
    }

    /// One context per household, acting as its first owner: what background
    /// tasks (schedulers, dispatchers) loop over.
    pub async fn household_contexts(&self) -> Result<Vec<HouseholdCtx>, AppError> {
        let rows = sqlx::query!(
            r#"SELECT m.household_id AS "household_id!", m.user_id AS "user_id!" FROM household_members m
               WHERE m.role = 'owner' AND m.created_at = (
                   SELECT min(o.created_at) FROM household_members o
                   WHERE o.household_id = m.household_id AND o.role = 'owner')
               GROUP BY m.household_id ORDER BY m.household_id"#,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(HouseholdCtx {
                    household_id: parse_stored::<HouseholdId>(&row.household_id)?,
                    user_id: parse_stored::<UserId>(&row.user_id)?,
                    role: Role::Owner,
                })
            })
            .collect()
    }
}
