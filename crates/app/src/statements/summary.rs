//! What a draft's summary holds, and small helpers for uploads.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::import_repo::LineRow;
use super::parse::{ParsedRow, StatementLine, Unsupported};

/// Most unreadable rows listed in a summary; the rest are only counted.
const MAX_LISTED_UNSUPPORTED: usize = 500;
/// Longest filename kept.
const MAX_FILENAME_CHARS: usize = 200;

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(super) fn split_rows(rows: Vec<ParsedRow>) -> (Vec<StatementLine>, Vec<(usize, Unsupported)>) {
    let mut lines = Vec::new();
    let mut unsupported = Vec::new();
    for row in rows {
        match row {
            ParsedRow::Line(line) => lines.push(line),
            ParsedRow::Unsupported { row, reason } => unsupported.push((row, reason)),
        }
    }
    (lines, unsupported)
}

/// Why a row is not a transaction, as the review shows it.
fn reason_text(reason: &Unsupported) -> String {
    match reason {
        Unsupported::NotATransaction => "not a transaction".to_string(),
        Unsupported::Pending => "pending: the bank has not booked it yet".to_string(),
        Unsupported::NothingMoved => "nothing moved".to_string(),
        Unsupported::OtherCurrency(code) => format!("in another currency ({code})"),
        Unsupported::Skipped(reason) => reason.clone(),
        Unsupported::UnreadableDate(text) => format!("unreadable date {text:?}"),
        Unsupported::UnreadableAmount(text) => format!("unreadable amount {text:?}"),
    }
}

pub(super) fn summary(
    rows: &[LineRow],
    unsupported: &[(usize, Unsupported)],
    findings: Vec<Value>,
    part_accounts: Value,
) -> Value {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for row in rows {
        *counts.entry(row.status.as_str()).or_insert(0) += 1;
    }
    let listed: Vec<Value> = unsupported
        .iter()
        .take(MAX_LISTED_UNSUPPORTED)
        .map(|(row, reason)| json!({ "row": row, "reason": reason_text(reason) }))
        .collect();
    json!({
        "counts": counts,
        "unsupported": listed,
        "unsupported_total": unsupported.len(),
        "findings": findings,
        "part_accounts": part_accounts,
    })
}

/// The file's own name: no folders, no control characters.
pub(super) fn clean_filename(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let clean: String = base
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_FILENAME_CHARS)
        .collect();
    if clean.is_empty() {
        "statement".to_string()
    } else {
        clean
    }
}
