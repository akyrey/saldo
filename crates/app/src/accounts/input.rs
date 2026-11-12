//! Input types for accounts and the checks that apply to a whole row.

use saldo_core::{Account, AccountType, CurrencyCode, EquityRole, Money, UserId};
use sqlx::SqliteConnection;

use super::repo::AccountRow;
use crate::ctx::HouseholdCtx;
use crate::currencies;
use crate::error::{AppError, parse_stored};

pub const CREATABLE_TYPES: [&str; 4] = ["asset", "expense", "revenue", "liability"];
pub const ALL_TYPES: [&str; 5] = ["asset", "expense", "revenue", "liability", "equity"];
const ASSET_ROLES: [&str; 5] = ["default", "shared", "savings", "credit_card", "cash_wallet"];
const LIABILITY_TYPES: [&str; 3] = ["loan", "debt", "mortgage"];
const LIABILITY_DIRECTIONS: [&str; 2] = ["i_owe", "owed_to_me"];
const INTEREST_PERIODS: [&str; 6] = [
    "daily",
    "weekly",
    "monthly",
    "quarterly",
    "half_year",
    "yearly",
];

/// A new account. `currency` defaults to the household's primary currency.
#[derive(Debug, Clone, Default)]
pub struct NewAccount {
    pub name: String,
    /// `asset`, `expense`, `revenue` or `liability`. Equity accounts are
    /// created by the system only.
    pub account_type: String,
    pub owner_user_id: Option<UserId>,
    pub currency: Option<String>,
    pub asset_role: Option<String>,
    pub liability_type: Option<String>,
    pub liability_direction: Option<String>,
    pub interest_rate: Option<String>,
    pub interest_period: Option<String>,
    pub iban: Option<String>,
    pub bic: Option<String>,
    pub account_number: Option<String>,
    /// Decimal string in the account's currency.
    pub virtual_balance: Option<String>,
    pub include_net_worth: Option<bool>,
    pub active: Option<bool>,
    pub sort_order: Option<i64>,
    pub notes: Option<String>,
}

/// Changes to an account. `None` keeps a value; for nullable fields
/// `Some(None)` clears it. Type and currency cannot change.
#[derive(Debug, Clone, Default)]
pub struct AccountUpdate {
    pub name: Option<String>,
    /// Accepted only if equal to the current type.
    pub account_type: Option<String>,
    pub owner_user_id: Option<Option<UserId>>,
    pub asset_role: Option<Option<String>>,
    pub liability_type: Option<Option<String>>,
    pub liability_direction: Option<Option<String>>,
    pub interest_rate: Option<Option<String>>,
    pub interest_period: Option<Option<String>>,
    pub iban: Option<Option<String>>,
    pub bic: Option<Option<String>>,
    pub account_number: Option<Option<String>>,
    pub virtual_balance: Option<String>,
    pub include_net_worth: Option<bool>,
    pub active: Option<bool>,
    pub sort_order: Option<i64>,
    pub notes: Option<Option<String>>,
}

/// Returns `row` with `update` applied; the original is left untouched.
pub(crate) fn apply_update(
    current: &AccountRow,
    update: AccountUpdate,
) -> Result<AccountRow, AppError> {
    // `is_some_and` checks the value only if there is one.
    if update
        .account_type
        .as_ref()
        .is_some_and(|t| *t != current.account_type)
    {
        return Err(AppError::invalid(
            "type",
            "an account's type cannot be changed",
        ));
    }
    let row = current.clone();
    Ok(AccountRow {
        name: update.name.unwrap_or(row.name),
        owner_user_id: update
            .owner_user_id
            .map_or(row.owner_user_id, |o| o.map(|u| u.to_string())),
        asset_role: update.asset_role.unwrap_or(row.asset_role),
        liability_type: update.liability_type.unwrap_or(row.liability_type),
        liability_direction: update
            .liability_direction
            .unwrap_or(row.liability_direction),
        interest_rate: update.interest_rate.unwrap_or(row.interest_rate),
        interest_period: update.interest_period.unwrap_or(row.interest_period),
        iban: update.iban.unwrap_or(row.iban),
        bic: update.bic.unwrap_or(row.bic),
        account_number: update.account_number.unwrap_or(row.account_number),
        include_net_worth: update.include_net_worth.unwrap_or(row.include_net_worth),
        active: update.active.unwrap_or(row.active),
        sort_order: update.sort_order.unwrap_or(row.sort_order),
        notes: update.notes.unwrap_or(row.notes),
        ..row
    })
}

/// Checks the rules that span several fields, before writing `row`.
pub(crate) async fn check_row(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    row: &AccountRow,
) -> Result<(), AppError> {
    if row.name.trim().is_empty() {
        return Err(AppError::invalid("name", "must not be empty"));
    }
    let is_asset = row.account_type == "asset";
    let is_liability = row.account_type == "liability";
    check_choice(
        "asset_role",
        row.asset_role.as_deref(),
        &ASSET_ROLES,
        is_asset,
    )?;
    check_choice(
        "liability_type",
        row.liability_type.as_deref(),
        &LIABILITY_TYPES,
        is_liability,
    )?;
    check_choice(
        "liability_direction",
        row.liability_direction.as_deref(),
        &LIABILITY_DIRECTIONS,
        is_liability,
    )?;
    check_choice(
        "interest_period",
        row.interest_period.as_deref(),
        &INTEREST_PERIODS,
        is_liability,
    )?;
    let bad_rate = |rate: &String| !is_liability || !is_decimal(rate);
    if row.interest_rate.as_ref().is_some_and(bad_rate) {
        return Err(AppError::invalid(
            "interest_rate",
            "must be a decimal number on a liability",
        ));
    }
    if let Some(owner) = &row.owner_user_id {
        let household_id = ctx.household_id.to_string();
        let member = sqlx::query_scalar!(
            "SELECT EXISTS (SELECT 1 FROM household_members WHERE household_id = ? AND user_id = ?)",
            household_id,
            owner,
        )
        .fetch_one(&mut *conn)
        .await?;
        if member == 0 {
            return Err(AppError::invalid(
                "owner_user_id",
                "is not a member of this household",
            ));
        }
    }
    Ok(())
}

/// A value from a fixed list, only allowed on one account type.
fn check_choice(
    field: &str,
    value: Option<&str>,
    allowed: &[&str],
    type_allows: bool,
) -> Result<(), AppError> {
    let Some(value) = value else {
        return Ok(());
    };
    if !type_allows {
        return Err(AppError::invalid(
            field,
            "is not allowed on this account type",
        ));
    }
    if !allowed.contains(&value) {
        return Err(AppError::invalid(
            field,
            format!("must be one of {}", allowed.join(", ")),
        ));
    }
    Ok(())
}

fn is_decimal(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, "0"));
    let all_digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    all_digits(whole) && all_digits(fraction)
}

/// Parses a virtual balance in the account's currency into minor units.
pub(crate) async fn parse_virtual_balance(
    conn: &mut SqliteConnection,
    currency_code: &str,
    text: &str,
) -> Result<i64, AppError> {
    let currency = currencies::find(conn, currency_code, "currency").await?;
    Money::parse_decimal(text, &currency)
        .map(|money| money.minor())
        .map_err(|e| AppError::invalid("virtual_balance", e.to_string()))
}

/// The domain view of a stored account, for validation in `saldo-core`.
pub(crate) fn core_account(row: &AccountRow) -> Result<Account, AppError> {
    let account_type = match (row.account_type.as_str(), row.equity_role.as_deref()) {
        ("asset", _) => AccountType::Asset,
        ("expense", _) => AccountType::Expense,
        ("revenue", _) => AccountType::Revenue,
        ("liability", _) => AccountType::Liability,
        ("equity", Some("opening_balance")) => AccountType::Equity(EquityRole::OpeningBalance),
        ("equity", Some("reconciliation")) => AccountType::Equity(EquityRole::Reconciliation),
        (other, role) => {
            return Err(AppError::Internal(format!(
                "bad account type {other} {role:?}"
            )));
        }
    };
    Ok(Account {
        id: parse_stored(&row.id)?,
        household_id: parse_stored(&row.household_id)?,
        owner: row.owner_user_id.as_deref().map(parse_stored).transpose()?,
        name: row.name.clone(),
        account_type,
        currency: CurrencyCode::new(&row.currency_code)
            .map_err(|e| AppError::Internal(e.to_string()))?,
        active: row.active,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_are_plain_numbers() {
        for good in ["4", "4.5", "-0.25", "10.000"] {
            assert!(is_decimal(good), "{good}");
        }
        for bad in ["", "4.", ".5", "4,5", "1e3", "abc"] {
            assert!(!is_decimal(bad), "{bad}");
        }
    }

    #[test]
    fn choices_must_be_listed_and_allowed_on_the_type() {
        assert!(check_choice("f", None, &ASSET_ROLES, false).is_ok());
        assert!(check_choice("f", Some("savings"), &ASSET_ROLES, true).is_ok());
        assert!(check_choice("f", Some("savings"), &ASSET_ROLES, false).is_err());
        assert!(check_choice("f", Some("piggy"), &ASSET_ROLES, true).is_err());
    }
}
