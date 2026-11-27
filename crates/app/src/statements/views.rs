//! What the statement import returns and accepts.

use saldo_core::{Currency, Money};
use serde::Serialize;
use serde_json::Value;

use super::import_repo::{ImportRow, LineRow};
use crate::error::AppError;

/// An upload: a file read with a saved profile.
#[derive(Debug, Clone)]
pub struct UploadInput {
    /// The account the file belongs to; the profile's account when `None`.
    pub account_id: Option<String>,
    pub profile_id: String,
    pub filename: String,
    pub bytes: Vec<u8>,
}

/// One import without its lines (lists, events).
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct ImportView {
    pub id: String,
    pub account_id: String,
    pub profile_id: Option<String>,
    pub filename: String,
    /// `draft`, `committed`, `discarded` or `reverted`.
    pub status: String,
    pub currency: String,
    pub opening_balance: Option<String>,
    pub closing_balance: Option<String>,
    pub closing_date: Option<String>,
    /// Opening balance plus the lines minus the closing balance; not zero
    /// means the file is missing lines or holds pending ones.
    pub gap: Option<String>,
    /// `counts` per status, `unsupported` rows `{ row, reason }`, and
    /// `findings` about existing transactions.
    #[schema(value_type = Object)]
    pub summary: Value,
    pub created_by_user_id: Option<String>,
    pub created_at: String,
    pub committed_at: Option<String>,
}

/// One part of a line paid from another account (brief decision 13).
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct PartView {
    pub column: String,
    pub amount: String,
}

/// One importable line of a statement.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct LineView {
    pub position: i64,
    /// The row in the file, counting from 1.
    pub row: i64,
    pub accounting_date: String,
    pub value_date: String,
    /// The date the transaction gets.
    pub booked_date: String,
    /// Signed for the account.
    pub amount: String,
    pub description: String,
    pub category: Option<String>,
    pub counterparty_name: Option<String>,
    pub counterparty_iban: Option<String>,
    pub external_id: Option<String>,
    /// The line's identity across imports.
    pub ref_key: String,
    pub notes: Vec<String>,
    pub parts: Vec<PartView>,
    /// `new`, `duplicate`, `possible_match`, `skipped_before` or
    /// `not_matched_reconciled`.
    pub status: String,
    /// `create`, `link` or `skip`.
    pub action: String,
    pub reason: Option<String>,
    /// The matched split (link), or the first split created (after commit).
    pub split_id: Option<String>,
    pub created_group_id: Option<String>,
    /// The proposed or chosen counterparty; `None` creates an expense or
    /// revenue account named after the counterparty on commit.
    pub counterparty_account_id: Option<String>,
    pub category_id: Option<String>,
}

/// An import with its lines, in file order.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct ImportDetail {
    pub import: ImportView,
    pub lines: Vec<LineView>,
}

/// A review edit of a draft line. `None` keeps a value; for the nullable
/// fields `Some(None)` clears it.
#[derive(Debug, Clone, Default)]
pub struct LineUpdate {
    pub action: Option<String>,
    pub split_id: Option<Option<String>>,
    pub counterparty_account_id: Option<Option<String>>,
    pub category_id: Option<Option<String>>,
}

fn money(minor: i64, currency: &Currency) -> String {
    Money::new(minor, currency.code()).to_decimal_string(currency)
}

pub(super) fn import_view(row: ImportRow, currency: &Currency) -> Result<ImportView, AppError> {
    let summary = serde_json::from_str(&row.summary)
        .map_err(|e| AppError::Internal(format!("stored summary: {e}")))?;
    Ok(ImportView {
        id: row.id,
        account_id: row.account_id,
        profile_id: row.profile_id,
        filename: row.filename,
        status: row.status,
        currency: row.currency_code,
        opening_balance: row.opening_balance_minor.map(|m| money(m, currency)),
        closing_balance: row.closing_balance_minor.map(|m| money(m, currency)),
        closing_date: row.closing_date,
        gap: row.gap_minor.map(|m| money(m, currency)),
        summary,
        created_by_user_id: row.created_by_user_id,
        created_at: row.created_at,
        committed_at: row.committed_at,
    })
}

pub(super) fn line_view(row: LineRow, currency: &Currency) -> Result<LineView, AppError> {
    let bad = |e: serde_json::Error| AppError::Internal(format!("stored line: {e}"));
    let notes: Vec<String> = serde_json::from_str(&row.notes).map_err(bad)?;
    let parts: Vec<Value> = serde_json::from_str(&row.parts).map_err(bad)?;
    let parts = parts
        .iter()
        .map(|p| PartView {
            column: p["column"].as_str().unwrap_or_default().to_string(),
            amount: money(p["amount_minor"].as_i64().unwrap_or_default(), currency),
        })
        .collect();
    Ok(LineView {
        position: row.position,
        row: row.row_number,
        accounting_date: row.accounting_date,
        value_date: row.value_date,
        booked_date: row.booked_date,
        amount: money(row.amount_minor, currency),
        description: row.description,
        category: row.category,
        counterparty_name: row.counterparty_name,
        counterparty_iban: row.counterparty_iban,
        external_id: row.external_id,
        ref_key: row.ref_key,
        notes,
        parts,
        status: row.status,
        action: row.action,
        reason: row.reason,
        split_id: row.split_id,
        created_group_id: row.created_group_id,
        counterparty_account_id: row.counterparty_account_id,
        category_id: row.category_id,
    })
}

pub(super) fn detail_of(
    row: ImportRow,
    lines: Vec<LineRow>,
    currency: &Currency,
) -> Result<ImportDetail, AppError> {
    Ok(ImportDetail {
        import: import_view(row, currency)?,
        lines: lines
            .into_iter()
            .map(|l| line_view(l, currency))
            .collect::<Result<_, _>>()?,
    })
}
