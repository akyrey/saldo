//! The JSON shape of an account, used by the API and in events.

use serde::Serialize;
use sqlx::SqliteConnection;

use super::repo::AccountRow;
use crate::currencies;
use crate::error::AppError;
use saldo_core::Money;

/// An account as the API returns it. Amounts are decimal strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct AccountView {
    pub id: String,
    pub name: String,
    /// `asset`, `expense`, `revenue`, `liability` or `equity`.
    #[serde(rename = "type")]
    pub account_type: String,
    /// `opening_balance` or `reconciliation`, only for equity accounts.
    pub equity_role: Option<String>,
    /// `None` means a joint account.
    pub owner_user_id: Option<String>,
    pub currency: String,
    pub asset_role: Option<String>,
    pub liability_type: Option<String>,
    pub liability_direction: Option<String>,
    pub interest_rate: Option<String>,
    pub interest_period: Option<String>,
    pub iban: Option<String>,
    pub bic: Option<String>,
    pub account_number: Option<String>,
    pub virtual_balance: String,
    pub include_net_worth: bool,
    pub active: bool,
    pub sort_order: i64,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Builds the view of a stored row (the currency gives the decimal places).
pub(crate) async fn view_of(
    conn: &mut SqliteConnection,
    row: AccountRow,
) -> Result<AccountView, AppError> {
    let currency = currencies::find(conn, &row.currency_code, "currency").await?;
    let virtual_balance =
        Money::new(row.virtual_balance_minor, currency.code()).to_decimal_string(&currency);
    Ok(AccountView {
        id: row.id,
        name: row.name,
        account_type: row.account_type,
        equity_role: row.equity_role,
        owner_user_id: row.owner_user_id,
        currency: row.currency_code,
        asset_role: row.asset_role,
        liability_type: row.liability_type,
        liability_direction: row.liability_direction,
        interest_rate: row.interest_rate,
        interest_period: row.interest_period,
        iban: row.iban,
        bic: row.bic,
        account_number: row.account_number,
        virtual_balance,
        include_net_worth: row.include_net_worth,
        active: row.active,
        sort_order: row.sort_order,
        notes: row.notes,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
