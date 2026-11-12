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
