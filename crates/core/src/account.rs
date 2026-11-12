//! Accounts.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::currency::CurrencyCode;
use crate::ids::{AccountId, HouseholdId, UserId};

/// What kind of account this is. Never changes after creation.
///
/// A Rust enum is a closed set of cases like a PHP 8.1 enum, but a case can
/// carry data: `Equity` holds which equity account it is. (Go would need an
/// interface with one struct per case.) In JSON the plain cases are strings
/// such as `"asset"` and the equity case is `{"equity": "opening_balance"}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountType {
    Asset,
    Expense,
    Revenue,
    Liability,
    /// A system account; each household has one per role.
    Equity(EquityRole),
}

/// Which system equity account: the source of opening balances, or the
/// counterpart of reconciliation corrections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquityRole {
    OpeningBalance,
    Reconciliation,
}

impl AccountType {
    /// Asset and liability accounts are the ones that hold *your* money.
    pub fn is_asset_or_liability(self) -> bool {
        matches!(self, AccountType::Asset | AccountType::Liability)
    }
}

impl fmt::Display for AccountType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            AccountType::Asset => "asset",
            AccountType::Expense => "expense",
            AccountType::Revenue => "revenue",
            AccountType::Liability => "liability",
            AccountType::Equity(EquityRole::OpeningBalance) => "opening balance",
            AccountType::Equity(EquityRole::Reconciliation) => "reconciliation",
        };
        f.write_str(name)
    }
}

/// An account as the domain needs it. Storage-only details (IBAN, asset role,
/// liability terms, sort order) live in `saldo-app`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub household_id: HouseholdId,
    /// `None` means a joint account. `Option<T>` is PHP's `?T` or a Go pointer
    /// that may be nil, except the compiler makes you handle the `None` case.
    pub owner: Option<UserId>,
    pub name: String,
    pub account_type: AccountType,
    pub currency: CurrencyCode,
    pub active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn asset_and_liability_hold_money() {
        assert!(AccountType::Asset.is_asset_or_liability());
        assert!(AccountType::Liability.is_asset_or_liability());
        assert!(!AccountType::Expense.is_asset_or_liability());
        assert!(!AccountType::Revenue.is_asset_or_liability());
        assert!(!AccountType::Equity(EquityRole::OpeningBalance).is_asset_or_liability());
    }

    #[test]
    fn serde_names_match_the_sql_values() {
        let cases = [
            (AccountType::Asset, r#""asset""#),
            (AccountType::Expense, r#""expense""#),
            (AccountType::Revenue, r#""revenue""#),
            (AccountType::Liability, r#""liability""#),
            (
                AccountType::Equity(EquityRole::OpeningBalance),
                r#"{"equity":"opening_balance"}"#,
            ),
            (
                AccountType::Equity(EquityRole::Reconciliation),
                r#"{"equity":"reconciliation"}"#,
            ),
        ];
        for (account_type, json) in cases {
            assert_eq!(serde_json::to_string(&account_type).unwrap(), json);
            assert_eq!(
                serde_json::from_str::<AccountType>(json).unwrap(),
                account_type
            );
        }
    }

    #[test]
    fn display_is_human_readable() {
        assert_eq!(AccountType::Asset.to_string(), "asset");
        assert_eq!(AccountType::Expense.to_string(), "expense");
        assert_eq!(AccountType::Revenue.to_string(), "revenue");
        assert_eq!(AccountType::Liability.to_string(), "liability");
        let opening = AccountType::Equity(EquityRole::OpeningBalance);
        assert_eq!(opening.to_string(), "opening balance");
        let reconciliation = AccountType::Equity(EquityRole::Reconciliation);
        assert_eq!(reconciliation.to_string(), "reconciliation");
    }

    #[test]
    fn account_round_trips_through_json() {
        let account = Account {
            id: AccountId(Uuid::from_u128(1)),
            household_id: HouseholdId(Uuid::from_u128(2)),
            owner: None,
            name: "Checking".to_string(),
            account_type: AccountType::Asset,
            currency: CurrencyCode::new("EUR").unwrap(),
            active: true,
        };
        let json = serde_json::to_string(&account).unwrap();
        assert_eq!(serde_json::from_str::<Account>(&json).unwrap(), account);
    }
}
