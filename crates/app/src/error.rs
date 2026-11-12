//! The error type of every service.

use std::str::FromStr;

use saldo_core::ValidationError;
use thiserror::Error;

/// Everything a service call can fail with. The server maps each variant to
/// an HTTP status and the API error envelope.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Conflict(String),
    /// Ledger rules from `saldo-core` were broken.
    #[error("validation failed")]
    Validation(Vec<ValidationError>),
    /// One input field is unusable (unknown currency, empty name, ...).
    #[error("{field}: {message}")]
    Invalid { field: String, message: String },
    #[error("authentication required")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    /// `#[from]` writes `impl From<sqlx::Error> for AppError`, so `?` on a
    /// database call converts the error automatically: Go's error wrapping,
    /// done by the type system.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn invalid(field: impl Into<String>, message: impl Into<String>) -> Self {
        AppError::Invalid {
            field: field.into(),
            message: message.into(),
        }
    }

    /// Turns a UNIQUE constraint failure into a `Conflict` with `message`;
    /// any other database error stays a database error.
    pub fn conflict_on_unique(error: sqlx::Error, message: &str) -> Self {
        let unique = error
            .as_database_error()
            .is_some_and(|db| db.is_unique_violation());
        if unique {
            AppError::Conflict(message.to_string())
        } else {
            AppError::Database(error)
        }
    }
}

/// Parses an ID read back from the database. A failure means a corrupt row,
/// not bad user input, hence `Internal`.
pub(crate) fn parse_stored<T: FromStr>(text: &str) -> Result<T, AppError> {
    text.parse()
        .map_err(|_| AppError::Internal(format!("corrupt value in database: {text:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use saldo_core::AccountId;

    #[test]
    fn stored_ids_parse_or_report_corruption() {
        let id: AccountId = parse_stored("0190f5c3-7a2b-7c00-8000-000000000001").unwrap();
        assert_eq!(id.to_string(), "0190f5c3-7a2b-7c00-8000-000000000001");
        let err = parse_stored::<AccountId>("garbage").unwrap_err();
        assert!(matches!(err, AppError::Internal(_)));
    }

    #[test]
    fn non_unique_errors_stay_database_errors() {
        let err = AppError::conflict_on_unique(sqlx::Error::RowNotFound, "dup");
        assert!(matches!(err, AppError::Database(_)));
    }

    #[test]
    fn invalid_names_the_field() {
        assert_eq!(
            AppError::invalid("name", "must not be empty").to_string(),
            "name: must not be empty"
        );
    }
}
