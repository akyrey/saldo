//! One module per Firefly entity, each with the same signature:
//! `pub async fn import_<entity>(cx: &ImportCx, report: &mut ImportReport) -> Result<(), ImportError>`.

pub mod accounts;
pub mod attachments;
pub mod bills;
pub mod budgets;
pub mod categories;
pub mod currencies;
pub mod rules;
pub mod tags;
pub mod transactions;

use saldo_app::error::AppError;

use crate::engine::ImportError;

/// Problems with one item (bad data, a conflict) skip that item; anything
/// else (database, network) stops the import.
pub(crate) fn skippable(error: AppError) -> Result<String, ImportError> {
    match error {
        AppError::Invalid { .. }
        | AppError::Conflict(_)
        | AppError::Validation(_)
        | AppError::NotFound => Ok(describe(&error)),
        other => Err(other.into()),
    }
}

fn describe(error: &AppError) -> String {
    match error {
        AppError::Validation(errors) => errors
            .iter()
            .map(|e| e.code())
            .collect::<Vec<_>>()
            .join(", "),
        other => other.to_string(),
    }
}
