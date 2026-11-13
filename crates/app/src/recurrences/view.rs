//! The JSON shape of a recurrence, used by the API and in events.

use saldo_core::Money;
use serde::Serialize;
use sqlx::SqliteConnection;

use super::repo::{self, RecurrenceRow, TemplateSplitRow};
use crate::currencies;
use crate::error::AppError;
use crate::transactions::recurring::last_fired_date;

#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct RecurrenceView {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub first_date: String,
    pub repeat_until: Option<String>,
    pub max_occurrences: Option<i64>,
    /// Whether rules run on the transactions it creates.
    pub apply_rules: bool,
    pub active: bool,
    pub notes: Option<String>,
    /// The runner has handled every date up to this one.
    pub fired_through: Option<String>,
    /// The latest date a transaction was created for.
    pub last_fired_date: Option<String>,
    pub repetitions: Vec<RepetitionView>,
    pub splits: Vec<TemplateSplitView>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct RepetitionView {
    /// `daily`, `weekly`, `monthly`, `nth_weekday` or `yearly`.
    pub kind: String,
    /// `''` daily; `1`-`7` (Mon-Sun) weekly; `1`-`31` monthly;
    /// `week,weekday` nth_weekday; `MM-DD` yearly.
    pub moment: String,
    pub skip: i64,
    /// `keep`, `skip`, `previous_friday` or `next_monday`.
    pub weekend_policy: String,
}

/// One template split. Amounts are decimal strings in their currency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct TemplateSplitView {
    pub position: i64,
    pub description: String,
    pub source_account_id: String,
    pub destination_account_id: String,
    pub amount: String,
    pub currency: String,
    pub foreign_amount: Option<String>,
    pub foreign_currency: Option<String>,
    pub category_id: Option<String>,
    pub budget_id: Option<String>,
    pub bill_id: Option<String>,
    pub piggy_bank_id: Option<String>,
    pub tag_ids: Vec<String>,
}

pub(super) async fn view_of(
    conn: &mut SqliteConnection,
    row: RecurrenceRow,
) -> Result<RecurrenceView, AppError> {
    let repetitions = repo::repetitions(&mut *conn, &row.household_id, &row.id)
        .await?
        .into_iter()
        .map(|r| RepetitionView {
            kind: r.kind,
            moment: r.moment,
            skip: r.skip,
            weekend_policy: r.weekend_policy,
        })
        .collect();
    let mut splits = Vec::new();
    for split in repo::splits(&mut *conn, &row.household_id, &row.id).await? {
        splits.push(split_view(&mut *conn, split).await?);
    }
    let last_fired_date = last_fired_date(&mut *conn, &row.household_id, &row.id).await?;
    Ok(RecurrenceView {
        id: row.id,
        title: row.title,
        description: row.description,
        first_date: row.first_date,
        repeat_until: row.repeat_until,
        max_occurrences: row.max_occurrences,
        apply_rules: row.apply_rules,
        active: row.active,
        notes: row.notes,
        fired_through: row.fired_through,
        last_fired_date,
        repetitions,
        splits,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// A stored amount as a decimal string in its currency.
pub(super) async fn decimal(
    conn: &mut SqliteConnection,
    minor: i64,
    code: &str,
) -> Result<String, AppError> {
    let currency = currencies::find(&mut *conn, code, "currency").await?;
    Ok(Money::new(minor, currency.code()).to_decimal_string(&currency))
}

async fn split_view(
    conn: &mut SqliteConnection,
    row: TemplateSplitRow,
) -> Result<TemplateSplitView, AppError> {
    let foreign_amount = match (row.foreign_amount_minor, &row.foreign_currency_code) {
        (Some(minor), Some(code)) => Some(decimal(&mut *conn, minor, code).await?),
        _ => None,
    };
    let tag_ids: Vec<String> = serde_json::from_str(&row.tags)
        .map_err(|e| AppError::Internal(format!("corrupt template tags: {e}")))?;
    Ok(TemplateSplitView {
        amount: decimal(&mut *conn, row.amount_minor, &row.currency_code).await?,
        position: row.position,
        description: row.description,
        source_account_id: row.source_account_id,
        destination_account_id: row.destination_account_id,
        currency: row.currency_code,
        foreign_amount,
        foreign_currency: row.foreign_currency_code,
        category_id: row.category_id,
        budget_id: row.budget_id,
        bill_id: row.bill_id,
        piggy_bank_id: row.piggy_bank_id,
        tag_ids,
    })
}
