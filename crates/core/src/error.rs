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
