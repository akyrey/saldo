//! The JSON shape of a piggy bank, used by the API and in events.

use saldo_core::{Currency, Money};
use serde::Serialize;
use sqlx::SqliteConnection;

use super::repo::{self, PiggyBankRow};
use crate::currencies;
use crate::error::AppError;

/// A piggy bank. Amounts are decimal strings in its currency.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct PiggyBankView {
    pub id: String,
    pub name: String,
    pub currency: String,
    pub target_amount: Option<String>,
    /// Sum of all events.
    pub saved_amount: String,
    /// Target minus saved, never below zero; `None` without a target.
    pub left_to_save: Option<String>,
    pub start_date: Option<String>,
    pub target_date: Option<String>,
    pub active: bool,
    pub sort_order: i64,
    pub notes: Option<String>,
    /// The linked accounts with the amount saved in each.
    pub accounts: Vec<PiggyBankAccountView>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct PiggyBankAccountView {
    pub account_id: String,
    pub saved_amount: String,
}

pub(crate) fn decimal(minor: i64, currency: &Currency) -> String {
    Money::new(minor, currency.code()).to_decimal_string(currency)
}

pub(super) async fn view_of(
    conn: &mut SqliteConnection,
    row: PiggyBankRow,
) -> Result<PiggyBankView, AppError> {
    let currency = currencies::find(&mut *conn, &row.currency_code, "currency").await?;
    let accounts = repo::accounts(&mut *conn, &row.household_id, &row.id).await?;
    let (saved, _) = repo::saved(&mut *conn, &row.id, "").await?;
    let left = row
        .target_amount_minor
        .map(|target| decimal(target.saturating_sub(saved).max(0), &currency));
    Ok(PiggyBankView {
        id: row.id,
        name: row.name,
        target_amount: row.target_amount_minor.map(|t| decimal(t, &currency)),
        saved_amount: decimal(saved, &currency),
        left_to_save: left,
        currency: row.currency_code,
        start_date: row.start_date,
        target_date: row.target_date,
        active: row.active,
        sort_order: row.sort_order,
        notes: row.notes,
        accounts: accounts
            .into_iter()
            .map(|(account_id, saved)| PiggyBankAccountView {
                account_id,
                saved_amount: decimal(saved, &currency),
            })
            .collect(),
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
