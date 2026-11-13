//! Groups created by a recurring transaction: the same validation and event
//! as any other group, plus `recurrence_id` and `recurrence_date`, whose
//! UNIQUE pair makes firing idempotent.

use chrono::NaiveDate;
use saldo_core::{GroupId, ValidatedGroup};
use sqlx::SqliteConnection;

use super::{TransactionGroupView, TransactionInput, TransactionService, prepare};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::AppError;

/// Which recurrence fires, for which date, and whether its rules run.
pub(crate) struct Firing<'a> {
    pub recurrence_id: &'a str,
    pub date: NaiveDate,
    pub apply_rules: bool,
}

impl TransactionService {
    /// Creates the group that `recurrence_id` fires on `date`, unless one
    /// exists already, running the rules only if `apply_rules`. Returns the
    /// group and whether it was created now.
    pub(crate) async fn create_for_recurrence_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: TransactionInput,
        firing: Firing<'_>,
    ) -> Result<(TransactionGroupView, bool), AppError> {
        let Firing {
            recurrence_id,
            date,
            apply_rules,
        } = firing;
        let household_id = ctx.household_id.to_string();
        if let Some(existing) = fired_group(&mut *conn, &household_id, recurrence_id, date).await? {
            let view = self.get_in(&mut *conn, ctx, existing).await?;
            return Ok((view, false));
        }
        let view = self.write_create_in(&mut *conn, ctx, origin, input).await?;
        let date = date.to_string();
        sqlx::query!(
            "UPDATE transaction_groups SET recurrence_id = ?, recurrence_date = ?
             WHERE household_id = ? AND id = ?",
            recurrence_id,
            date,
            household_id,
            view.id,
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| {
            AppError::conflict_on_unique(e, "the recurrence already fired on this date")
        })?;
        if !apply_rules {
            return Ok((view, true));
        }
        let view = self.run_rules(conn, ctx, origin, "create", view).await?;
        Ok((view, true))
    }
}

/// The group `recurrence_id` created on `date`, if any.
pub(crate) async fn fired_group(
    conn: &mut SqliteConnection,
    household_id: &str,
    recurrence_id: &str,
    date: NaiveDate,
) -> Result<Option<GroupId>, AppError> {
    let date = date.to_string();
    let id = sqlx::query_scalar!(
        "SELECT id FROM transaction_groups
         WHERE household_id = ? AND recurrence_id = ? AND recurrence_date = ?",
        household_id,
        recurrence_id,
        date,
    )
    .fetch_optional(&mut *conn)
    .await?;
    id.map(|id| crate::error::parse_stored(&id)).transpose()
}

/// The latest date `recurrence_id` created a group for.
pub(crate) async fn last_fired_date(
    conn: &mut SqliteConnection,
    household_id: &str,
    recurrence_id: &str,
) -> Result<Option<String>, AppError> {
    let date = sqlx::query_scalar!(
        "SELECT max(recurrence_date) FROM transaction_groups
         WHERE household_id = ? AND recurrence_id = ?",
        household_id,
        recurrence_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(date)
}

/// Checks a recurrence's template splits with the ledger rules, as if they
/// were entered as a group (used when a recurrence is saved).
pub(crate) async fn validate_template(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    origin: Origin,
    input: TransactionInput,
) -> Result<ValidatedGroup, AppError> {
    prepare::validate(conn, ctx, origin, input).await
}
