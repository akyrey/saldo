//! The JSON shapes of reconciliations, used by the API and in events.

use saldo_core::Money;
use serde::Serialize;
use sqlx::SqliteConnection;

use super::repo::{self, ReconciliationRow};
use crate::currencies;
use crate::error::AppError;

/// A finished reconciliation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct ReconciliationView {
    pub id: String,
    pub account_id: String,
    pub statement_date: String,
    pub statement_balance: String,
    pub currency: String,
    /// What the statement had more than the confirmed splits ("0.00" when
    /// they agreed); booked as the `difference_group_id` transaction.
    pub difference: String,
    pub difference_group_id: Option<String>,
    /// The splits it confirmed, the difference's included.
    pub split_ids: Vec<String>,
    pub created_by_user_id: Option<String>,
    pub created_at: String,
}

/// What reconciling up to a date starts from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct ReconciliationDraftView {
    pub account_id: String,
    pub currency: String,
    pub statement_date: String,
    /// The latest reconciliation's statement date, if any.
    pub reconciled_through: Option<String>,
    /// Reconciled splits and opening balances up to the date.
    pub cleared_balance: String,
    /// The splits up to the date the bank has not confirmed yet, oldest first.
    pub splits: Vec<DraftSplitView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct DraftSplitView {
    pub id: String,
    pub group_id: String,
    pub date: String,
    pub description: String,
    /// Positive when money came into the account.
    pub amount: String,
}

pub(super) async fn view_of(
    conn: &mut SqliteConnection,
    row: ReconciliationRow,
) -> Result<ReconciliationView, AppError> {
    let currency = currencies::find(conn, &row.currency_code, "currency").await?;
    let money = |minor| Money::new(minor, currency.code()).to_decimal_string(&currency);
    Ok(ReconciliationView {
        split_ids: repo::linked(conn, &row.id).await?,
        statement_balance: money(row.statement_balance_minor),
        difference: money(row.difference_minor),
        id: row.id,
        account_id: row.account_id,
        statement_date: row.statement_date,
        currency: row.currency_code,
        difference_group_id: row.difference_group_id,
        created_by_user_id: row.created_by_user_id,
        created_at: row.created_at,
    })
}
