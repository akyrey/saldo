//! Which splits an update keeps (ARCHITECTURE.md 6): a split input that
//! names the ID of one of the group's splits continues it, keeping its ID
//! and attachments; the group's splits nobody names are dropped.

use std::collections::HashSet;

use saldo_core::SplitId;
use sqlx::SqliteConnection;

use super::{TransactionInput, repo};
use crate::error::AppError;

/// What an update does with the group's current splits.
pub(crate) struct SplitPlan {
    /// For each input split, the split it continues (`None`: a new one).
    pub kept: Vec<Option<SplitId>>,
    /// The group's splits that the update leaves out.
    pub dropped: Vec<String>,
}

/// A new group has no splits to continue, so naming an ID is a mistake.
pub(crate) fn reject_ids(input: &TransactionInput) -> Result<(), AppError> {
    match input.splits.iter().position(|split| split.id.is_some()) {
        Some(index) => Err(AppError::invalid(
            format!("splits[{index}].id"),
            "only an update can name an existing split",
        )),
        None => Ok(()),
    }
}

/// Checks that every named ID is a split of this group, named once.
pub(crate) async fn plan(
    conn: &mut SqliteConnection,
    household_id: &str,
    group_id: &str,
    input: &TransactionInput,
) -> Result<SplitPlan, AppError> {
    let current = repo::split_ids(conn, household_id, group_id).await?;
    let mut named = HashSet::new();
    for (index, split) in input.splits.iter().enumerate() {
        let Some(id) = split.id else { continue };
        let id = id.to_string();
        let field = format!("splits[{index}].id");
        if !current.contains(&id) {
            return Err(AppError::invalid(field, "not a split of this transaction"));
        }
        if !named.insert(id) {
            return Err(AppError::invalid(field, "names the same split twice"));
        }
    }
    let dropped = current
        .into_iter()
        .filter(|id| !named.contains(id))
        .collect();
    Ok(SplitPlan {
        kept: input.splits.iter().map(|split| split.id).collect(),
        dropped,
    })
}
