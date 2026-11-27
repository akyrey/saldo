//! Turning a statement line into the splits of its transaction: the
//! accounts it moves, the proposal's details, splitting by item (Phase 13).

use std::collections::BTreeMap;

use saldo_core::matching::split_by_item;
use saldo_core::{AccountId, Currency, Money};
use serde_json::{Map, Value, json};

use super::enrich::Proposal;
use super::import_repo::{ImportRow, LineRow};
use crate::error::{AppError, parse_stored};
use crate::external_refs::ExternalRefs;
use crate::transactions::SplitInput;

/// The part column → account map the draft was reviewed with, kept in its
/// summary at upload (decision 13).
pub(super) fn part_accounts(row: &ImportRow) -> Result<BTreeMap<String, Option<String>>, AppError> {
    let summary: Value = serde_json::from_str(&row.summary)
        .map_err(|e| AppError::Internal(format!("stored summary: {e}")))?;
    match summary.get("part_accounts") {
        Some(value) if !value.is_null() => serde_json::from_value(value.clone())
            .map_err(|e| AppError::Internal(format!("stored summary: {e}"))),
        _ => Ok(BTreeMap::new()),
    }
}

/// Refuses a line another import booked since this draft was made (two
/// drafts of one file, or a duplicate forced to create or link), and a link
/// to a split already linked from this account's statements: either would
/// book a line twice or overwrite a link a later revert would then lose.
pub(super) fn still_unbooked(
    refs: &ExternalRefs,
    account_id: &str,
    line: &LineRow,
) -> Result<(), AppError> {
    if refs.resolve("split", &line.ref_key).is_some() {
        return Err(AppError::Conflict(format!(
            "row {} is already imported; upload the file again to review it",
            line.row_number
        )));
    }
    if line.action == "link"
        && let Some(split) = &line.split_id
    {
        let prefix = format!("{account_id}:");
        let taken = refs
            .entries("split")
            .iter()
            .any(|(key, local)| local == split && key.starts_with(&prefix));
        if taken {
            return Err(AppError::Conflict(format!(
                "row {} links to a transaction already linked to another line of this account",
                line.row_number
            )));
        }
    }
    Ok(())
}

/// The line's proposal when the owner keeps it.
pub(super) fn proposal_of(line: &LineRow) -> Result<Option<Proposal>, AppError> {
    match (&line.proposal, line.use_proposal) {
        (Some(text), true) => serde_json::from_str(text)
            .map(Some)
            .map_err(|e| AppError::Internal(format!("stored proposal: {e}"))),
        _ => Ok(None),
    }
}

/// `(account, signed minor, description)` per split: one per move, or with
/// "split by item" one per item of a single Amazon charge (decision 9). The
/// difference to the items goes to the largest; when that cannot work (a
/// gift card covered more than it), the charge stays one split.
pub(super) fn by_item(
    booked: &LineRow,
    proposal: Option<&Proposal>,
    moves: Moves,
    currency: &Currency,
) -> Vec<(String, i64, String)> {
    let single = |moves: Moves| {
        moves
            .into_iter()
            .map(|(a, m)| (a, m, booked.description.clone()))
            .collect()
    };
    let items = proposal.map(|p| p.items.as_slice()).unwrap_or_default();
    let [(account, minor)] = moves.as_slice() else {
        return single(moves);
    };
    if !booked.split_by_item || items.is_empty() {
        return single(moves);
    }
    let code = currency.code();
    let amounts: Vec<Money> = items
        .iter()
        .map(|i| Money::new(i.amount_minor, code))
        .collect();
    let Some(parts) = split_by_item(Money::new(minor.saturating_abs(), code), &amounts) else {
        return single(moves);
    };
    let sign = if *minor < 0 { -1 } else { 1 };
    parts
        .iter()
        .zip(items)
        .map(|(part, item)| (account.clone(), sign * part.minor(), item.product.clone()))
        .collect()
}

/// `(account ID, signed minor units)` for each account a line moves.
pub(super) type Moves = Vec<(String, i64)>;

/// Who pays what for one line: `(account, signed minor)` for the line's
/// own amount and each mapped part, and the notes (the bank text, the
/// parser's notes, unmapped parts). One line is one movement, so all moves
/// must go the same way.
pub(super) fn moves_of(
    import: &ImportRow,
    line: &LineRow,
    currency: &Currency,
    part_accounts: &BTreeMap<String, Option<String>>,
) -> Result<(Moves, Vec<String>), AppError> {
    let bad = |e: serde_json::Error| AppError::Internal(format!("stored line: {e}"));
    let parts: Vec<Value> = serde_json::from_str(&line.parts).map_err(bad)?;
    let mut notes = vec![format!("Bank: {}", line.description)];
    notes.extend(serde_json::from_str::<Vec<String>>(&line.notes).map_err(bad)?);
    let mut moves: Moves = Vec::new();
    if line.amount_minor != 0 {
        moves.push((import.account_id.clone(), line.amount_minor));
    }
    for part in &parts {
        let column = part["column"].as_str().unwrap_or_default();
        let minor = part["amount_minor"].as_i64().unwrap_or_default();
        let mapped = part_accounts
            .iter()
            .find(|(name, _)| name.trim().eq_ignore_ascii_case(column.trim()))
            .and_then(|(_, account)| account.clone());
        match mapped {
            Some(account) => moves.push((account, minor)),
            None => notes.push(format!(
                "{column}: {}",
                Money::new(minor, currency.code()).to_decimal_string(currency)
            )),
        }
    }
    let row = line.row_number;
    let Some((_, first)) = moves.first() else {
        return Err(AppError::invalid(
            "lines",
            format!("row {row} has nothing to book: map its part columns to accounts"),
        ));
    };
    if moves.iter().any(|(_, minor)| (*minor < 0) != (*first < 0)) {
        return Err(AppError::invalid(
            "lines",
            format!("row {row} mixes money in and out; skip it and enter it by hand"),
        ));
    }
    Ok((moves, notes))
}

/// `meta.statement` of a created split (decision 8): where it came from.
pub(super) fn statement_meta(import: &ImportRow, line: &LineRow) -> Map<String, Value> {
    let mut meta = Map::new();
    meta.insert(
        "statement".to_string(),
        json!({
            "import_id": import.id,
            "position": line.position,
            "ref_key": line.ref_key,
            "accounting_date": line.accounting_date,
            "value_date": line.value_date,
        }),
    );
    meta
}

pub(super) fn split_input(
    line: &LineRow,
    currency: &Currency,
    source: &str,
    destination: &str,
    minor: i64,
    notes: Option<String>,
    meta: Map<String, Value>,
) -> Result<SplitInput, AppError> {
    let source: AccountId = parse_stored(source)?;
    let destination: AccountId = parse_stored(destination)?;
    Ok(SplitInput {
        id: None,
        date: line
            .booked_date
            .parse()
            .map_err(|_| AppError::Internal(format!("stored date {}", line.booked_date)))?,
        description: line.description.clone(),
        source_id: source,
        destination_id: destination,
        amount: Money::new(minor.saturating_abs(), currency.code()).to_decimal_string(currency),
        currency: None,
        foreign_amount: None,
        foreign_currency: None,
        category_id: line.category_id.as_deref().map(parse_stored).transpose()?,
        budget_id: None,
        bill_id: None,
        tag_ids: Vec::new(),
        reconciled: false,
        notes,
        meta,
    })
}
