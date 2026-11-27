//! Checks shared by draft edits: the draft itself, a line edit, the
//! statement's account and format (Phase 13).

use sqlx::SqliteConnection;

use super::account_type;
use super::import_repo::{self, ImportRow, LineRow};
use super::parse::Format;
use super::views::LineUpdate;
use crate::currencies;
use crate::error::AppError;
use saldo_core::Currency;

/// The import, if it is still a draft (409 otherwise).
pub(super) async fn draft(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<ImportRow, AppError> {
    let row = import_repo::find(conn, household_id, id).await?;
    if row.status != "draft" {
        return Err(AppError::Conflict(format!(
            "the import is {}, not a draft",
            row.status
        )));
    }
    Ok(row)
}

pub(super) async fn apply_update(
    conn: &mut SqliteConnection,
    household_id: &str,
    account_id: &str,
    line: &mut LineRow,
    update: LineUpdate,
) -> Result<(), AppError> {
    if let Some(action) = update.action {
        if !matches!(action.as_str(), "create" | "link" | "skip") {
            return Err(AppError::invalid("action", "must be create, link or skip"));
        }
        line.action = action;
    }
    if let Some(split_id) = update.split_id {
        if let Some(id) = &split_id
            && !import_repo::split_on_account(conn, household_id, account_id, id).await?
        {
            return Err(AppError::invalid("split_id", "not a split of this account"));
        }
        line.split_id = split_id;
    }
    if let Some(counterparty) = update.counterparty_account_id {
        if let Some(id) = &counterparty
            && (id == account_id || account_type(conn, household_id, id).await?.is_none())
        {
            return Err(AppError::invalid(
                "counterparty_account_id",
                "not another account of this household",
            ));
        }
        line.counterparty_account_id = counterparty;
    }
    if let Some(category) = update.category_id {
        if let Some(id) = &category
            && !import_repo::category_exists(conn, household_id, id).await?
        {
            return Err(AppError::invalid(
                "category_id",
                "not a category of this household",
            ));
        }
        line.category_id = category;
    }
    if let Some(keep) = update.use_proposal {
        line.use_proposal = keep;
    }
    if let Some(split) = update.split_by_item {
        let has_items = line
            .proposal
            .as_deref()
            .and_then(|p| serde_json::from_str::<super::enrich::Proposal>(p).ok())
            .is_some_and(|p| !p.items.is_empty());
        if split && !has_items {
            return Err(AppError::invalid(
                "split_by_item",
                "only an Amazon charge with its items can be split",
            ));
        }
        line.split_by_item = split;
    }
    if line.action == "link" && line.split_id.is_none() {
        return Err(AppError::invalid(
            "split_id",
            "a link needs the split it links to",
        ));
    }
    Ok(())
}

/// The currency of the household's asset or liability account.
pub(super) async fn statement_currency(
    conn: &mut SqliteConnection,
    household_id: &str,
    account: &str,
) -> Result<Currency, AppError> {
    let row = sqlx::query!(
        "SELECT type AS kind, currency_code FROM accounts WHERE household_id = ? AND id = ?",
        household_id,
        account
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| AppError::invalid("account_id", "not an account of this household"))?;
    if row.kind != "asset" && row.kind != "liability" {
        return Err(AppError::invalid(
            "account_id",
            "statements belong to asset and liability accounts",
        ));
    }
    currencies::find(conn, &row.currency_code, "account_id").await
}

/// The parser's format for a stored or requested format name.
pub(super) fn stored_format(name: &str) -> Option<Format> {
    match name {
        "csv" => Some(Format::Csv),
        "xlsx" => Some(Format::Xlsx),
        _ => None,
    }
}
