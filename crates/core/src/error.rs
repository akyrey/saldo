//! Error types for `saldo-core`.
//!
//! `thiserror` derives the `std::error::Error` trait and the human-readable
//! message (`Display`) from the `#[error("...")]` attributes. Like a Go error
//! struct with an `Error() string` method, but generated for you.

use thiserror::Error;

/// A string that is not a valid UUID was given where an ID was expected.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid id: {input:?}")]
pub struct IdError {
    pub input: String,
}

/// A currency code or currency definition is not valid.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CurrencyError {
    #[error("currency code must be three uppercase ASCII letters, got {input:?}")]
    InvalidCode { input: String },
    #[error("decimal places must be between 0 and 8, got {places}")]
    InvalidDecimalPlaces { places: u8 },
}

/// Parsing or arithmetic on [`crate::money::Money`] failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MoneyError {
    #[error("not a decimal amount: {input:?}")]
    InvalidFormat { input: String },
    #[error("{input:?} has more than {allowed} decimal places")]
    TooManyDecimalPlaces { input: String, allowed: u8 },
    #[error("amount is too large")]
    Overflow,
    #[error("cannot combine amounts in {left} and {right}")]
    CurrencyMismatch {
        left: crate::currency::CurrencyCode,
        right: crate::currency::CurrencyCode,
    },
}

/// No split kind exists for this pair of account types (see ARCHITECTURE.md 4.1).
///
/// Written without `thiserror`: it treats a field called `source` as the
/// underlying cause of the error, which is not what `source` means here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidAccountPair {
    pub source: crate::account::AccountType,
    pub destination: crate::account::AccountType,
}

impl std::fmt::Display for InvalidAccountPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "money cannot move from an account of type {} to an account of type {}",
            self.source, self.destination
        )
    }
}

// An empty impl: `Error` only needs `Debug` and `Display`, which exist above.
impl std::error::Error for InvalidAccountPair {}

/// Something a split refers to, other than its accounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reference {
    Category(crate::ids::CategoryId),
    Budget(crate::ids::BudgetId),
    Bill(crate::ids::BillId),
    Tag(crate::ids::TagId),
}

impl std::fmt::Display for Reference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Reference::Category(id) => write!(f, "category {id}"),
            Reference::Budget(id) => write!(f, "budget {id}"),
            Reference::Bill(id) => write!(f, "bill {id}"),
            Reference::Tag(id) => write!(f, "tag {id}"),
        }
    }
}

/// One broken ledger rule (ARCHITECTURE.md 4.2). `split` is the zero-based
/// position of the split in the group.
///
/// The JSON form carries the same `code` as [`ValidationError::code`], for
/// example `{"code": "same_account", "split": 1}`.
#[derive(Debug, Clone, PartialEq, Eq, Error, serde::Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum ValidationError {
    #[error("a transaction group needs at least one split")]
    EmptyGroup,
    #[error("split {split}: amount must be greater than zero")]
    AmountNotPositive { split: usize },
    #[error("split {split}: source and destination are the same account")]
    SameAccount { split: usize },
    #[error("split {split}: account {account} does not exist")]
    UnknownAccount {
        split: usize,
        account: crate::ids::AccountId,
    },
    #[error("split {split}: account {account} belongs to another household")]
    AccountOutsideHousehold {
        split: usize,
        account: crate::ids::AccountId,
    },
    #[error("split {split}: {reference} does not exist in this household")]
    UnknownReference { split: usize, reference: Reference },
    #[error(
        "split {split}: money cannot move from an account of type {source_type} to an account of type {destination_type}"
    )]
    InvalidAccountPair {
        split: usize,
        source_type: crate::account::AccountType,
        destination_type: crate::account::AccountType,
    },
    #[error("split {split}: account {account} is inactive")]
    AccountInactive {
        split: usize,
        account: crate::ids::AccountId,
    },
    #[error("split {split}: amount is in {found}, expected {expected}")]
    CurrencyMismatch {
        split: usize,
        expected: crate::currency::CurrencyCode,
        found: crate::currency::CurrencyCode,
    },
    #[error("split {split}: foreign amount must be greater than zero")]
    ForeignAmountNotPositive { split: usize },
    #[error("split {split}: foreign amount must be in a different currency than the amount")]
    ForeignCurrencySameAsAmount { split: usize },
    #[error("split {split}: a transfer between currencies needs a foreign amount in {expected}")]
    ForeignAmountRequired {
        split: usize,
        expected: crate::currency::CurrencyCode,
    },
    #[error("split {split}: foreign amount is in {found}, expected {expected}")]
    ForeignCurrencyMismatch {
        split: usize,
        expected: crate::currency::CurrencyCode,
        found: crate::currency::CurrencyCode,
    },
}

impl ValidationError {
    /// A stable, machine-readable code for API responses.
    pub fn code(&self) -> &'static str {
        match self {
            ValidationError::EmptyGroup => "empty_group",
            ValidationError::AmountNotPositive { .. } => "amount_not_positive",
            ValidationError::SameAccount { .. } => "same_account",
            ValidationError::UnknownAccount { .. } => "unknown_account",
            ValidationError::AccountOutsideHousehold { .. } => "account_outside_household",
            ValidationError::UnknownReference { .. } => "unknown_reference",
            ValidationError::InvalidAccountPair { .. } => "invalid_account_pair",
            ValidationError::AccountInactive { .. } => "account_inactive",
            ValidationError::CurrencyMismatch { .. } => "currency_mismatch",
            ValidationError::ForeignAmountNotPositive { .. } => "foreign_amount_not_positive",
            ValidationError::ForeignCurrencySameAsAmount { .. } => {
                "foreign_currency_same_as_amount"
            }
            ValidationError::ForeignAmountRequired { .. } => "foreign_amount_required",
            ValidationError::ForeignCurrencyMismatch { .. } => "foreign_currency_mismatch",
        }
    }

    /// The zero-based index of the split this error is about, if any.
    pub fn split(&self) -> Option<usize> {
        match self {
            ValidationError::EmptyGroup => None,
            ValidationError::AmountNotPositive { split }
            | ValidationError::SameAccount { split }
            | ValidationError::UnknownAccount { split, .. }
            | ValidationError::AccountOutsideHousehold { split, .. }
            | ValidationError::UnknownReference { split, .. }
            | ValidationError::InvalidAccountPair { split, .. }
            | ValidationError::AccountInactive { split, .. }
            | ValidationError::CurrencyMismatch { split, .. }
            | ValidationError::ForeignAmountNotPositive { split }
            | ValidationError::ForeignCurrencySameAsAmount { split }
            | ValidationError::ForeignAmountRequired { split, .. }
            | ValidationError::ForeignCurrencyMismatch { split, .. } => Some(*split),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::AccountType;
    use crate::currency::CurrencyCode;
    use crate::ids::{AccountId, BillId, BudgetId, CategoryId, TagId};
    use uuid::Uuid;

    fn all_validation_errors() -> Vec<ValidationError> {
        let account = AccountId(Uuid::from_u128(7));
        let eur = CurrencyCode::new("EUR").unwrap();
        let usd = CurrencyCode::new("USD").unwrap();
        vec![
            ValidationError::EmptyGroup,
            ValidationError::AmountNotPositive { split: 1 },
            ValidationError::SameAccount { split: 1 },
            ValidationError::UnknownAccount { split: 1, account },
            ValidationError::AccountOutsideHousehold { split: 1, account },
            ValidationError::UnknownReference {
                split: 1,
                reference: Reference::Tag(TagId(Uuid::from_u128(8))),
            },
            ValidationError::InvalidAccountPair {
                split: 1,
                source_type: AccountType::Expense,
                destination_type: AccountType::Asset,
            },
            ValidationError::AccountInactive { split: 1, account },
            ValidationError::CurrencyMismatch {
                split: 1,
                expected: eur,
                found: usd,
            },
            ValidationError::ForeignAmountNotPositive { split: 1 },
            ValidationError::ForeignCurrencySameAsAmount { split: 1 },
            ValidationError::ForeignAmountRequired {
                split: 1,
                expected: usd,
            },
            ValidationError::ForeignCurrencyMismatch {
                split: 1,
                expected: usd,
                found: eur,
            },
        ]
    }

    #[test]
    fn json_code_matches_code_method() {
        for error in all_validation_errors() {
            let json = serde_json::to_value(&error).unwrap();
            assert_eq!(json["code"], error.code(), "{error:?}");
        }
    }

    #[test]
    fn every_split_error_carries_its_index() {
        for error in all_validation_errors() {
            let expected = match error {
                ValidationError::EmptyGroup => None,
                _ => Some(1),
            };
            assert_eq!(error.split(), expected, "{error:?}");
            assert!(!error.to_string().is_empty());
        }
    }

    #[test]
    fn messages_are_readable() {
        let error = ValidationError::InvalidAccountPair {
            split: 2,
            source_type: AccountType::Expense,
            destination_type: AccountType::Asset,
        };
        assert_eq!(
            error.to_string(),
            "split 2: money cannot move from an account of type expense to an account of type asset"
        );
        let json = serde_json::to_string(&ValidationError::SameAccount { split: 3 }).unwrap();
        assert_eq!(json, r#"{"code":"same_account","split":3}"#);
    }

    #[test]
    fn references_name_their_kind() {
        let id = Uuid::from_u128(5);
        assert!(
            Reference::Category(CategoryId(id))
                .to_string()
                .starts_with("category ")
        );
        assert!(
            Reference::Budget(BudgetId(id))
                .to_string()
                .starts_with("budget ")
        );
        assert!(Reference::Bill(BillId(id)).to_string().starts_with("bill "));
        assert!(Reference::Tag(TagId(id)).to_string().starts_with("tag "));
        let json = serde_json::to_value(Reference::Bill(BillId(id))).unwrap();
        assert_eq!(json["bill"], id.to_string());
    }
}
