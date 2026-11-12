//! SQL for the `accounts` table. Every function filters by household.

use sqlx::SqliteConnection;

use crate::error::AppError;

/// One `accounts` row exactly as stored.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AccountRow {
    pub id: String,
    pub household_id: String,
    pub owner_user_id: Option<String>,
    pub name: String,
    pub account_type: String,
    pub equity_role: Option<String>,
    pub asset_role: Option<String>,
    pub liability_type: Option<String>,
    pub liability_direction: Option<String>,
    pub interest_rate: Option<String>,
    pub interest_period: Option<String>,
    pub currency_code: String,
    pub iban: Option<String>,
    pub bic: Option<String>,
    pub account_number: Option<String>,
    pub virtual_balance_minor: i64,
    pub include_net_worth: bool,
    pub active: bool,
    pub sort_order: i64,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub(crate) async fn insert(conn: &mut SqliteConnection, row: &AccountRow) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO accounts (id, household_id, owner_user_id, name, type, equity_role,
            asset_role, liability_type, liability_direction, interest_rate, interest_period,
            currency_code, iban, bic, account_number, virtual_balance_minor, include_net_worth,
            active, sort_order, notes, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.household_id,
        row.owner_user_id,
        row.name,
        row.account_type,
        row.equity_role,
        row.asset_role,
        row.liability_type,
        row.liability_direction,
        row.interest_rate,
        row.interest_period,
        row.currency_code,
        row.iban,
        row.bic,
        row.account_number,
        row.virtual_balance_minor,
        row.include_net_worth,
        row.active,
        row.sort_order,
        row.notes,
        row.created_at,
        row.updated_at,
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| {
        AppError::conflict_on_unique(e, "an account of this type with this name exists")
    })?;
    Ok(())
}
