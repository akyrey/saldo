//! Deciding what each statement line is (Phase 13 Track B, brief section 6):
//! already imported, skipped before, an existing transaction, flagged in a
//! reconciled period, or new; plus the proposed counterparty and the
//! findings about existing transactions the statement does not explain.

use std::collections::BTreeSet;

use chrono::NaiveDate;
use saldo_core::matching::{
    Candidate, Finding, LagWindow, LedgerSplit, Line, fingerprint, ledger_candidates, match_ledger,
    occurrences,
};
use saldo_core::{AccountId, Currency, CurrencyCode, DateRange, Money, SplitKind};
use serde_json::{Value, json};

use super::TransferRule;
use super::enrich::Enrichment;
use super::import_repo::LineRow;
use super::ledger_repo::{AccountRow, LedgerRow};
use super::parse::StatementLine;
use crate::error::{AppError, parse_stored};
use crate::external_refs::ExternalRefs;

/// Days a bank line and an existing transaction may be apart (decision 10).
pub(super) const LEDGER_WINDOW_DAYS: u32 = 3;
/// A transfer found through a transfer rule, such as a Satispay top-up the
/// bank books up to a week later (decision 10).
pub(super) const TRANSFER_WINDOW: LagWindow = LagWindow {
    early_days: 2,
    late_days: 7,
};

/// What the classification needs from the database.
pub(super) struct Known<'a> {
    pub account_id: AccountId,
    pub currency: &'a Currency,
    pub refs: &'a ExternalRefs,
    pub skipped_before: &'a BTreeSet<String>,
    pub ledger: &'a [LedgerRow],
    pub reconciled_through: Option<NaiveDate>,
    pub accounts: &'a [AccountRow],
    pub transfer_rules: &'a [TransferRule],
    /// What a stored Amazon or PayPal row says about each line (Track D).
    pub enrichments: &'a [Option<Enrichment>],
}

/// The `ref_key`s of the lines, in order: `<account>:<external ID>`, or
/// `<account>:#<fingerprint>` when the bank gives no ID (L17). Every key
/// starts with the account, so a split linked to one account's statement
/// stays a candidate for the other side of a transfer.
pub(super) fn ref_keys(account: AccountId, lines: &[StatementLine]) -> Vec<String> {
    let keys: Vec<(NaiveDate, Money, &str)> = lines
        .iter()
        .map(|l| (l.accounting_date, l.amount, l.description.as_str()))
        .collect();
    let counts = occurrences(&keys);
    lines
        .iter()
        .zip(counts)
        .map(|(line, n)| match &line.external_id {
            Some(id) => format!("{account}:{id}"),
            None => {
                let print = fingerprint(
                    account,
                    line.accounting_date,
                    line.amount,
                    &line.description,
                    n,
                );
                format!("{account}:#{print}")
            }
        })
        .collect()
}

/// Rows to store for `lines`, and the findings.
pub(super) fn classify(
    lines: &[StatementLine],
    keys: &[String],
    known: &Known<'_>,
) -> Result<(Vec<LineRow>, Vec<Value>), AppError> {
    let mut rows: Vec<LineRow> = lines
        .iter()
        .zip(keys)
        .enumerate()
        .map(|(position, (line, key))| base_row(position, line, key, known))
        .collect::<Result<_, _>>()?;
    for row in &mut rows {
        if let Some(split) = known.refs.resolve("split", &row.ref_key) {
            mark(row, "duplicate", "skip", "imported before");
            row.split_id = Some(split.to_string());
        } else if known.skipped_before.contains(&row.ref_key) {
            mark(
                row,
                "skipped_before",
                "skip",
                "skipped in an earlier import",
            );
        }
    }
    let findings = match_existing(lines, &mut rows, known)?;
    for (row, line) in rows.iter_mut().zip(lines) {
        let reconciled = known
            .reconciled_through
            .is_some_and(|through| line.booked_date <= through);
        if row.status == "new" && reconciled {
            mark(
                row,
                "not_matched_reconciled",
                "skip",
                "no transaction matches, in a period already reconciled",
            );
        }
    }
    Ok((rows, findings))
}

fn mark(row: &mut LineRow, status: &str, action: &str, reason: &str) {
    row.status = status.to_string();
    row.action = action.to_string();
    row.reason = Some(reason.to_string());
}

fn base_row(
    position: usize,
    line: &StatementLine,
    key: &str,
    known: &Known<'_>,
) -> Result<LineRow, AppError> {
    let json = |value: Value| value.to_string();
    let parts: Vec<Value> = line
        .parts
        .iter()
        .map(|p| json!({ "column": p.column, "amount_minor": p.amount.minor() }))
        .collect();
    Ok(LineRow {
        position: i64::try_from(position).map_err(|e| AppError::Internal(e.to_string()))?,
        row_number: i64::try_from(line.row).map_err(|e| AppError::Internal(e.to_string()))?,
        accounting_date: line.accounting_date.to_string(),
        value_date: line.value_date.to_string(),
        booked_date: line.booked_date.to_string(),
        amount_minor: line.amount.minor(),
        currency_code: known.currency.code().to_string(),
        description: line.description.clone(),
        category: line.category.clone(),
        counterparty_name: line.counterparty.clone(),
        counterparty_iban: line.counterparty_iban.clone(),
        external_id: line.external_id.clone(),
        ref_key: key.to_string(),
        notes: json(json!(line.notes)),
        parts: json(Value::Array(parts)),
        status: "new".to_string(),
        action: "create".to_string(),
        reason: None,
        split_id: None,
        created_group_id: None,
        counterparty_account_id: propose_counterparty(line, known),
        category_id: None,
        proposal: proposal(known, position)
            .map(|p| serde_json::to_string(p).map_err(|e| AppError::Internal(e.to_string())))
            .transpose()?,
        use_proposal: true,
        split_by_item: false,
    })
}

/// Runs the ledger matcher on the lines still new and records the matches;
/// returns the findings (L11–L15).
fn match_existing(
    lines: &[StatementLine],
    rows: &mut [LineRow],
    known: &Known<'_>,
) -> Result<Vec<Value>, AppError> {
    let splits = ledger_splits(known.ledger)?;
    let candidates = ledger_candidates(&splits, known.account_id);
    // Only new lines with an own amount take part; their positions in `rows`.
    let open: Vec<usize> = (0..rows.len())
        .filter(|&i| rows[i].status == "new" && lines[i].amount.minor() != 0)
        .collect();
    let matcher_lines: Vec<Line> = open
        .iter()
        .map(|&i| matcher_line(&lines[i], i, known))
        .collect();
    let (Some(first), Some(last)) = (
        lines.iter().map(|l| l.booked_date).min(),
        lines.iter().map(|l| l.booked_date).max(),
    ) else {
        return Ok(Vec::new());
    };
    let range =
        DateRange::new(first, last).ok_or_else(|| AppError::Internal("empty range".to_string()))?;
    let result = match_ledger(&matcher_lines, &candidates, LEDGER_WINDOW_DAYS, range, &[]);
    for (line_index, candidate_index) in result.matched {
        let (row, candidate) = (&mut rows[open[line_index]], &candidates[candidate_index]);
        let days = (candidate.date - lines[open[line_index]].booked_date)
            .num_days()
            .abs();
        let unit = if days == 1 { "day" } else { "days" };
        let what = if candidate.splits.len() > 1 {
            format!(" (a group of {} splits)", candidate.splits.len())
        } else {
            String::new()
        };
        mark(
            row,
            "possible_match",
            "link",
            &format!("same amount, {days} {unit} apart{what}"),
        );
        row.split_id = candidate.splits.first().map(ToString::to_string);
    }
    Ok(result
        .findings
        .iter()
        .map(|f| finding_json(f, &candidates, known.currency))
        .collect())
}

fn matcher_line(line: &StatementLine, position: usize, known: &Known<'_>) -> Line {
    let mut other_dates = if line.accounting_date == line.booked_date {
        vec![]
    } else {
        vec![line.accounting_date]
    };
    // A matched Amazon order's date: the purchase may be booked on it (L9).
    if let Some(Some(enrichment)) = known.enrichments.get(position) {
        other_dates.extend(enrichment.order_dates.iter().copied());
    }
    Line {
        date: line.booked_date,
        other_dates,
        amount: line.amount,
        text: line.description.clone(),
        window: transfer_rule(line, known).map(|_| TRANSFER_WINDOW),
    }
}

fn ledger_splits(rows: &[LedgerRow]) -> Result<Vec<LedgerSplit>, AppError> {
    rows.iter()
        .map(|row| {
            let kind: SplitKind = serde_json::from_value(Value::String(row.kind.clone()))
                .map_err(|e| AppError::Internal(format!("stored split kind: {e}")))?;
            let currency = CurrencyCode::new(&row.currency_code)
                .map_err(|e| AppError::Internal(e.to_string()))?;
            let date = row
                .date
                .parse()
                .map_err(|_| AppError::Internal(format!("stored date {}", row.date)))?;
            Ok(LedgerSplit {
                id: parse_stored(&row.id)?,
                group: parse_stored(&row.group_id)?,
                kind,
                date,
                amount: Money::new(row.amount_minor, currency),
                source: parse_stored(&row.source_account_id)?,
                destination: parse_stored(&row.destination_account_id)?,
                description: row.description.clone(),
            })
        })
        .collect()
}

fn finding_json(finding: &Finding, candidates: &[Candidate], currency: &Currency) -> Value {
    let about = |index: usize| {
        let c = &candidates[index];
        (
            c.splits.iter().map(ToString::to_string).collect::<Vec<_>>(),
            c,
        )
    };
    let describe = |kind: &str, index: usize| {
        let (split_ids, c) = about(index);
        json!({
            "kind": kind,
            "split_ids": split_ids,
            "date": c.date.to_string(),
            "amount": c.amount.to_decimal_string(currency),
            "description": c.description,
        })
    };
    match finding {
        Finding::DuplicateOf { candidate, of } => {
            let mut value = describe("duplicate_of", *candidate);
            value["of_split_ids"] = json!(about(*of).0);
            value
        }
        Finding::Identical { candidates: group } => {
            let mut value = describe("identical", group[0]);
            value["split_ids"] = json!(group.iter().flat_map(|i| about(*i).0).collect::<Vec<_>>());
            value
        }
        Finding::NotOnStatement { candidate } => describe("not_on_statement", *candidate),
    }
}

/// The first transfer rule whose pattern the line's description contains.
fn transfer_rule<'a>(line: &StatementLine, known: &Known<'a>) -> Option<&'a TransferRule> {
    let text = line.description.to_lowercase();
    known
        .transfer_rules
        .iter()
        .find(|rule| text.contains(&rule.pattern.trim().to_lowercase()))
}

/// Whether a line takes money out: its own amount, or for a payment made
/// with parts alone (Satispay vouchers), its first part. Classification and
/// commit share it, so the proposed counterparty fits the booking.
pub(super) fn money_out(own_minor: i64, first_part_minor: Option<i64>) -> bool {
    own_minor < 0 || (own_minor == 0 && first_part_minor.unwrap_or(0) < 0)
}

fn same_iban(a: &str, b: &str) -> bool {
    let clean = |s: &str| {
        s.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .to_uppercase()
    };
    !a.trim().is_empty() && clean(a) == clean(b)
}

/// The proposed counterparty, in order: one of the household's own accounts
/// by IBAN (a transfer), a transfer rule, an expense or revenue account by
/// IBAN, then by exact name ignoring case. `None` means "create one".
fn proposal<'a>(known: &Known<'a>, position: usize) -> Option<&'a super::enrich::Proposal> {
    known
        .enrichments
        .get(position)
        .and_then(|e| e.as_ref())
        .map(|e| &e.proposal)
}

fn propose_counterparty(line: &StatementLine, known: &Known<'_>) -> Option<String> {
    let me = known.account_id.to_string();
    let by_iban = |kinds: &[&str]| {
        let iban = line.counterparty_iban.as_deref()?;
        known
            .accounts
            .iter()
            .find(|a| {
                a.id != me
                    && kinds.contains(&a.kind.as_str())
                    && a.iban
                        .as_deref()
                        .is_some_and(|other| same_iban(iban, other))
            })
            .map(|a| a.id.clone())
    };
    if let Some(own) = by_iban(&["asset", "liability"]) {
        return Some(own);
    }
    if let Some(rule) = transfer_rule(line, known) {
        // Several accounts (two Satispay accounts on one bank account): the
        // first is proposed and the review can pick another.
        return rule.account_ids.first().cloned();
    }
    let first_part = line.parts.first().map(|p| p.amount.minor());
    let kind = if money_out(line.amount.minor(), first_part) {
        "expense"
    } else {
        "revenue"
    };
    if let Some(external) = by_iban(&[kind]) {
        return Some(external);
    }
    // A proposal's counterparty (the PayPal merchant) is not used here: it
    // is resolved when committing, so turning the proposal off books the
    // bank's view.
    let name = line.counterparty.as_deref()?.trim();
    known
        .accounts
        .iter()
        .find(|a| a.kind == kind && a.name.trim().to_lowercase() == name.to_lowercase())
        .map(|a| a.id.clone())
}
