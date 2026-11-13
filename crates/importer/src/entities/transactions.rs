//! `/v1/transactions` → `transaction_groups`, `splits`, `split_tags`
//! (ARCHITECTURE.md 9.1, row 9).

use std::collections::{BTreeSet, HashMap, HashSet};

use chrono::NaiveDate;
use saldo_app::ctx::Origin;
use saldo_app::currencies;
use saldo_app::external_refs::ExternalRefs;
use saldo_app::tags;
use saldo_app::transactions::{SplitInput, TransactionGroupView, TransactionInput};
use saldo_core::GroupId;
use serde_json::{Map, Value};

use super::accounts::{CASH_EXPENSE, CASH_REVENUE, REF as ACCOUNT_REF};
use crate::amounts::normalize;
use crate::engine::{ImportCx, ImportError, State};
use crate::firefly::models::{Resource, Split, TransactionGroup};
use crate::firefly::{MAX_PASSES, check_complete};
use crate::report::ImportReport;

const ENTITY: &str = "transactions";
pub const GROUP_REF: &str = "transaction_group";
pub const SPLIT_REF: &str = "split";

/// Firefly split fields copied into `meta.firefly` when they have a value.
const META_KEYS: [&str; 19] = [
    "sepa_cc",
    "sepa_ct_op",
    "sepa_ct_id",
    "sepa_db",
    "sepa_country",
    "sepa_ep",
    "sepa_ci",
    "sepa_batch_id",
    "internal_reference",
    "external_id",
    "external_url",
    "original_source",
    "import_hash_v2",
    "interest_date",
    "book_date",
    "process_date",
    "due_date",
    "payment_date",
    "invoice_date",
];

/// What converting a group needs besides the group: refs, decimal places
/// per currency and tag IDs by name.
struct Lookups {
    places: HashMap<String, u8>,
    tags: HashMap<String, String>,
}

pub async fn import_transactions(
    cx: &ImportCx,
    report: &mut ImportReport,
) -> Result<(), ImportError> {
    let since = cx.since.map(|d| d.to_string());
    let mut query = vec![("type", "all")];
    if let Some(since) = &since {
        query.push(("start", since.as_str()));
    }
    let groups = fetch_groups(cx, &query).await?;

    let mut guard = cx.state.lock().await;
    let state = &mut *guard;
    let lookups = Lookups {
        places: currencies::all_in(&mut state.tx)
            .await?
            .into_iter()
            .map(|c| (c.code, c.decimal_places))
            .collect(),
        tags: tags::ids_by_name(&mut state.tx, &cx.ctx).await?,
    };
    for group in &groups {
        import_group(cx, state, report, &lookups, group).await?;
    }
    if since.is_none() {
        let fetched: HashSet<&str> = groups.iter().map(|g| g.id.as_str()).collect();
        for (external, _) in state.refs.entries(GROUP_REF) {
            if !fetched.contains(external.as_str()) {
                report.note("groups deleted in Firefly, kept in Saldo");
            }
        }
    }
    Ok(())
}

/// All groups, complete. Firefly pages transactions by split, so a group
/// with several splits can be cut in two by a page boundary and arrive as two
/// parts with the same ID; the parts are merged by journal ID. If splits are
/// still missing (data changed while reading), the listing is repeated.
async fn fetch_groups(
    cx: &ImportCx,
    query: &[(&str, &str)],
) -> Result<Vec<Resource<TransactionGroup>>, ImportError> {
    let mut groups: Vec<Resource<TransactionGroup>> = Vec::new();
    let mut expected = 0;
    for _ in 0..MAX_PASSES {
        let (parts, total) = cx
            .firefly
            .list_with_total::<TransactionGroup>("/v1/transactions", query)
            .await?;
        expected = total;
        groups = merge_parts(groups, parts);
        if split_count(&groups) as u64 >= total {
            return Ok(groups);
        }
    }
    check_complete("/v1/transactions", expected, split_count(&groups))?;
    Ok(groups)
}

fn split_count(groups: &[Resource<TransactionGroup>]) -> usize {
    groups.iter().map(|g| g.attributes.transactions.len()).sum()
}

/// Adds `parts` to `groups`: a known group gains the splits it lacked.
fn merge_parts(
    groups: Vec<Resource<TransactionGroup>>,
    parts: Vec<Resource<TransactionGroup>>,
) -> Vec<Resource<TransactionGroup>> {
    let mut merged = groups;
    let mut index: HashMap<String, usize> = merged
        .iter()
        .enumerate()
        .map(|(i, g)| (g.id.clone(), i))
        .collect();
    for part in parts {
        match index.get(&part.id) {
            Some(&i) => {
                let group = &mut merged[i].attributes;
                for split in part.attributes.transactions {
                    if !group
                        .transactions
                        .iter()
                        .any(|s| s.transaction_journal_id == split.transaction_journal_id)
                    {
                        group.transactions.push(split);
                    }
                }
            }
            None => {
                index.insert(part.id.clone(), merged.len());
                merged.push(part);
            }
        }
    }
    merged
}

async fn import_group(
    cx: &ImportCx,
    state: &mut State,
    report: &mut ImportReport,
    lookups: &Lookups,
    group: &Resource<TransactionGroup>,
) -> Result<(), ImportError> {
    let input = match to_input(&state.refs, lookups, &group.attributes, report) {
        Ok(input) => input,
        Err(why) => {
            report.skipped(ENTITY, format!("group {}: {why}", group.id));
            return Ok(());
        }
    };
    let service = &cx.app.transactions;
    let existing = state
        .refs
        .resolve(GROUP_REF, &group.id)
        .and_then(|id| id.parse::<GroupId>().ok());
    let current = match existing {
        Some(id) => service.get_in(&mut state.tx, &cx.ctx, id).await.ok(),
        None => None,
    };
    let old_split_ids: HashSet<String> = current
        .iter()
        .flat_map(|v| v.splits.iter().map(|s| s.id.clone()))
        .collect();
    let result = match (existing, current) {
        (_, Some(view)) if same(&input, &view) => {
            note_kinds(report, &group.attributes, &view);
            report.unchanged(ENTITY);
            return Ok(());
        }
        (Some(id), Some(_)) => service
            .replace_in(&mut state.tx, &cx.ctx, Origin::Import, id, input)
            .await
            .map(|v| (v, false)),
        _ => service
            .create_in(&mut state.tx, &cx.ctx, Origin::Import, input)
            .await
            .map(|v| (v, true)),
    };
    let (view, created) = match result {
        Ok(done) => done,
        Err(error) => {
            report.skipped(
                ENTITY,
                format!("group {}: {}", group.id, super::skippable(error)?),
            );
            return Ok(());
        }
    };
    let now = cx.now();
    state
        .refs
        .upsert(&mut state.tx, GROUP_REF, &group.id, &view.id, &now)
        .await?;
    let journals: Vec<&str> = group
        .attributes
        .transactions
        .iter()
        .map(|s| s.transaction_journal_id.as_str())
        .collect();
    for (journal, split) in journals.iter().zip(&view.splits) {
        state
            .refs
            .upsert(&mut state.tx, SPLIT_REF, journal, &split.id, &now)
            .await?;
    }
    // Splits that disappeared from the Firefly group lose their reference.
    for (external, local) in state.refs.entries(SPLIT_REF) {
        if old_split_ids.contains(&local) && !journals.contains(&external.as_str()) {
            state
                .refs
                .remove(&mut state.tx, SPLIT_REF, &external)
                .await?;
        }
    }
    note_kinds(report, &group.attributes, &view);
    if created {
        report.created(ENTITY)
    } else {
        report.updated(ENTITY)
    }
    Ok(())
}

/// Firefly's group as Saldo input. `Err` explains why it cannot be imported.
fn to_input(
    refs: &ExternalRefs,
    lookups: &Lookups,
    group: &TransactionGroup,
    report: &mut ImportReport,
) -> Result<TransactionInput, String> {
    let splits = group
        .transactions
        .iter()
        .map(|split| to_split(refs, lookups, split, report))
        .collect::<Result<Vec<_>, String>>()?;
    let title = group
        .group_title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string);
    Ok(TransactionInput { title, splits })
}

fn to_split(
    refs: &ExternalRefs,
    lookups: &Lookups,
    s: &Split,
    report: &mut ImportReport,
) -> Result<SplitInput, String> {
    let account = |cash_ref: &str, id: &str| {
        refs.resolve(cash_ref, id)
            .or_else(|| refs.resolve(ACCOUNT_REF, id))
            .and_then(|local| local.parse().ok())
            .ok_or_else(|| format!("account {id} was not imported"))
    };
    let places = |code: &str| {
        lookups
            .places
            .get(code)
            .copied()
            .ok_or_else(|| format!("unknown currency {code}"))
    };
    let date_text = s.date.get(..10).unwrap_or_default();
    let date = NaiveDate::parse_from_str(date_text, "%Y-%m-%d")
        .map_err(|_| format!("bad date {:?}", s.date))?;
    let amount = normalize(&s.amount, places(&s.currency_code)?)?;
    let (foreign_amount, foreign_currency) = foreign(s, places, report)?;

    let category_id = match &s.category_id {
        Some(id) => match refs.resolve("category", id).and_then(|c| c.parse().ok()) {
            Some(local) => Some(local),
            None => {
                report.note("category not imported, split kept without it");
                None
            }
        },
        None => None,
    };
    let mut tag_ids = Vec::new();
    for name in &s.tags {
        match lookups.tags.get(name.trim()).and_then(|id| id.parse().ok()) {
            Some(id) => tag_ids.push(id),
            None => report.note("tag not imported, split kept without it"),
        }
    }
    let linked = |key: &str| s.other.get(key).is_some_and(|v| !v.is_null());
    if linked("budget_id") {
        report.note("budget link not imported (Phase 5)");
    }
    if linked("subscription_id") || linked("bill_id") {
        report.note("subscription link not imported (Phase 5)");
    }
    Ok(SplitInput {
        date,
        description: s.description.trim().to_string(),
        // Cash is the expense side when money leaves, the revenue side when it arrives.
        source_id: account(CASH_REVENUE, &s.source_id)?,
        destination_id: account(CASH_EXPENSE, &s.destination_id)?,
        amount,
        currency: Some(s.currency_code.clone()),
        foreign_amount,
        foreign_currency,
        category_id,
        budget_id: None,
        bill_id: None,
        tag_ids,
        reconciled: s.reconciled,
        notes: s
            .notes
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string),
        meta: meta(s),
    })
}

/// The foreign amount, unless it is missing, zero, or in the split's own
/// currency (Firefly ignores those too; Saldo's rule 7 forbids the last).
fn foreign(
    s: &Split,
    places: impl Fn(&str) -> Result<u8, String>,
    report: &mut ImportReport,
) -> Result<(Option<String>, Option<String>), String> {
    let (Some(amount), Some(code)) = (&s.foreign_amount, &s.foreign_currency_code) else {
        return Ok((None, None));
    };
    if *code == s.currency_code {
        report.note("foreign amount in the split's own currency dropped");
        return Ok((None, None));
    }
    let amount = normalize(amount, places(code)?)?;
    if amount.trim_start_matches(['0', '.']).is_empty() {
        report.note("zero foreign amount dropped");
        return Ok((None, None));
    }
    Ok((Some(amount), Some(code.clone())))
}

/// `{ "firefly": { "journal_id", "date", ...fields with a value } }`
fn meta(s: &Split) -> Map<String, Value> {
    let mut firefly = Map::new();
    firefly.insert(
        "journal_id".into(),
        Value::String(s.transaction_journal_id.clone()),
    );
    firefly.insert("date".into(), Value::String(s.date.clone()));
    for key in META_KEYS {
        match s.other.get(key) {
            None | Some(Value::Null) => {}
            Some(Value::String(text)) if text.is_empty() => {}
            Some(value) => {
                firefly.insert(key.to_string(), value.clone());
            }
        }
    }
    let mut meta = Map::new();
    meta.insert("firefly".into(), Value::Object(firefly));
    meta
}

/// True if the stored group already equals what the input would write.
fn same(input: &TransactionInput, view: &TransactionGroupView) -> bool {
    input.title == view.title
        && input.splits.len() == view.splits.len()
        && input.splits.iter().zip(&view.splits).all(|(s, v)| {
            let tags: BTreeSet<String> = s.tag_ids.iter().map(|t| t.to_string()).collect();
            let stored: BTreeSet<String> = v.tag_ids.iter().cloned().collect();
            s.date.to_string() == v.date
                && s.description == v.description
                && s.source_id.to_string() == v.source_account_id
                && s.destination_id.to_string() == v.destination_account_id
                && s.amount == v.amount
                && s.currency.as_deref() == Some(v.currency.as_str())
                && s.foreign_amount == v.foreign_amount
                && s.foreign_currency == v.foreign_currency
                && s.category_id.map(|c| c.to_string()) == v.category_id
                && v.budget_id.is_none()
                && v.bill_id.is_none()
                && tags == stored
                && s.reconciled == v.reconciled
                && s.notes == v.notes
                && Value::Object(s.meta.clone()) == v.meta
        })
}

/// Counts splits whose Firefly type differs from the kind Saldo derived.
fn note_kinds(report: &mut ImportReport, group: &TransactionGroup, view: &TransactionGroupView) {
    for (s, v) in group.transactions.iter().zip(&view.splits) {
        let expected = s.split_type.replace(' ', "_");
        if expected != v.kind {
            report.note(&format!(
                "kind: Firefly {} is Saldo {}",
                s.split_type, v.kind
            ));
        }
    }
}
