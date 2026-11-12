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
