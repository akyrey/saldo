//! `/v1/recurrences` → `recurrences`, `recurrence_repetitions`,
//! `recurrence_splits`.
//!
//! Firefly already created the transactions of past dates (they arrive with
//! the transactions import), so a new recurrence is marked as handled up to
//! Firefly's `latest_date`, or today when Firefly never fired it.

use std::collections::{BTreeSet, HashMap};

use saldo_app::ctx::Origin;
use saldo_app::identity::today_in;
use saldo_app::recurrences::{
    RecurrenceInput, RecurrenceView, RepetitionInput, TemplateSplitInput,
};
use saldo_app::tags;
use saldo_core::{Pattern, RecurrenceId};

use super::accounts::{CASH_EXPENSE, CASH_REVENUE, REF as ACCOUNT_REF};
use super::piggy_banks::{REF as PIGGY_REF, date_part, places};
use crate::amounts::normalize;
use crate::engine::{ImportCx, ImportError, State};
use crate::firefly::planning_models::{Recurrence, RecurrenceRepetition, RecurrenceTransaction};
use crate::report::ImportReport;

const ENTITY: &str = "recurrences";
pub const REF: &str = "recurrence";
/// Reference types the budget and bill importers record (other Phase 5
/// tracks); until they exist these links are dropped and reported.
const BUDGET_REF: &str = "budget";
const BILL_REF: &str = "bill";

/// What the conversion needs besides the Firefly object.
struct Lookups {
    places: HashMap<String, u8>,
    tags: HashMap<String, String>,
}

pub async fn import_recurrences(
    cx: &ImportCx,
    report: &mut ImportReport,
) -> Result<(), ImportError> {
    let items = cx
        .firefly
        .list::<Recurrence>("/v1/recurrences", &[])
        .await?;
    let mut guard = cx.state.lock().await;
    let state = &mut *guard;
    let lookups = Lookups {
        places: places(state).await?,
        tags: tags::ids_by_name(&mut state.tx, &cx.ctx).await?,
    };
    let today = today_in(&mut state.tx, &cx.ctx, &*cx.app.clock).await?;
    for item in items {
        let title = item.attributes.title.clone();
        let input = match to_input(state, &lookups, &item.attributes, report) {
            Ok(input) => input,
            Err(why) => {
                report.skipped(ENTITY, format!("{title:?}: {why}"));
                continue;
            }
        };
        let input = in_account_currency(cx, state, input, report).await?;
        let existing = state
            .refs
            .resolve(REF, &item.id)
            .and_then(|id| id.parse::<RecurrenceId>().ok());
        let service = &cx.app.recurrences;
        let current = match existing {
            Some(id) => service.get_in(&mut state.tx, &cx.ctx, id).await.ok(),
            None => None,
        };
        let result = match current {
            Some(view) if same(&view, &input) => {
                report.unchanged(ENTITY);
                continue;
            }
            Some(view) => {
                let id = parse_id(&view.id)?;
                service
                    .replace_in(&mut state.tx, &cx.ctx, Origin::Import, id, input)
                    .await
                    .map(|v| (v.id, false))
            }
            None => service
                .create_in(&mut state.tx, &cx.ctx, Origin::Import, input)
                .await
                .map(|v| (v.id, true)),
        };
        let (local_id, created) = match result {
            Ok(done) => done,
            Err(error) => {
                let why = super::skippable(error)?;
                report.skipped(ENTITY, format!("{title:?}: {why}"));
                continue;
            }
        };
        if created {
            let handled = item
                .attributes
                .latest_date
                .as_deref()
                .and_then(date_part)
                .unwrap_or(today);
            service
                .mark_fired_through_in(&mut state.tx, &cx.ctx, parse_id(&local_id)?, handled)
                .await?;
            report.created(ENTITY);
        } else {
            report.updated(ENTITY);
        }
        state
            .refs
            .upsert(&mut state.tx, REF, &item.id, &local_id, &cx.now())
            .await?;
    }
    Ok(())
}

fn parse_id(text: &str) -> Result<RecurrenceId, ImportError> {
    text.parse()
        .map_err(|_| ImportError::Invalid(format!("bad id {text}")))
}

fn to_input(
    state: &State,
    lookups: &Lookups,
    item: &Recurrence,
    report: &mut ImportReport,
) -> Result<RecurrenceInput, String> {
    let first_date = date_part(&item.first_date)
        .ok_or_else(|| format!("bad first date {:?}", item.first_date))?;
    let repetitions = item
        .repetitions
        .iter()
        .map(repetition)
        .collect::<Result<Vec<_>, String>>()?;
    let splits = item
        .transactions
        .iter()
        .map(|t| split(state, lookups, t, report))
        .collect::<Result<Vec<_>, String>>()?;
    Ok(RecurrenceInput {
        title: item.title.trim().to_string(),
        description: item.description.clone().filter(|d| !d.trim().is_empty()),
        first_date,
        repeat_until: item.repeat_until.as_deref().and_then(date_part),
        max_occurrences: item.nr_of_repetitions.filter(|n| *n > 0),
        apply_rules: Some(item.apply_rules.unwrap_or(true)),
        active: Some(item.active.unwrap_or(true)),
        notes: item.notes.clone().filter(|n| !n.trim().is_empty()),
        repetitions,
        splits,
    })
}

/// Firefly's repetition in Saldo's terms: `ndom` is `nth_weekday`, a yearly
/// moment is a full date (`2026-03-15` → `03-15`), weekend 1-4 is keep,
/// skip, previous Friday, next Monday.
fn repetition(r: &RecurrenceRepetition) -> Result<RepetitionInput, String> {
    let kind = match r.kind.as_str() {
        "ndom" => "nth_weekday",
        other => other,
    };
    let moment = match kind {
        "yearly" if r.moment.len() == 10 => r.moment[5..].to_string(),
        _ => r.moment.clone(),
    };
    let weekend = match r.weekend.unwrap_or(1) {
        1 => "keep",
        2 => "skip",
        3 => "previous_friday",
        4 => "next_monday",
        other => return Err(format!("unknown weekend value {other}")),
    };
    Ok(RepetitionInput {
        kind: kind.to_string(),
        moment,
        skip: r.skip,
        weekend_policy: Some(weekend.to_string()),
    })
}

/// Firefly lets a template's amount be in another currency than its asset
/// or liability account, with the account's currency as the foreign amount.
/// Saldo books a split in its account's currency, so such splits are swapped.
async fn in_account_currency(
    cx: &ImportCx,
    state: &mut State,
    input: RecurrenceInput,
    report: &mut ImportReport,
) -> Result<RecurrenceInput, ImportError> {
    let mut splits = Vec::with_capacity(input.splits.len());
    for split in input.splits.clone() {
        splits.push(swapped_if_needed(cx, state, split, report).await?);
    }
    Ok(RecurrenceInput { splits, ..input })
}

async fn swapped_if_needed(
    cx: &ImportCx,
    state: &mut State,
    split: TemplateSplitInput,
    report: &mut ImportReport,
) -> Result<TemplateSplitInput, ImportError> {
    let (Some(foreign_amount), Some(foreign_currency)) =
        (split.foreign_amount.clone(), split.foreign_currency.clone())
    else {
        return Ok(split);
    };
    let accounts = &cx.app.accounts;
    let source = accounts
        .get_in(&mut state.tx, &cx.ctx, split.source_id)
        .await?;
    let home = if matches!(source.account_type.as_str(), "asset" | "liability") {
        source
    } else {
        accounts
            .get_in(&mut state.tx, &cx.ctx, split.destination_id)
            .await?
    };
    if split.currency.as_deref() == Some(home.currency.as_str())
        || home.currency != foreign_currency
    {
        return Ok(split);
    }
    report.note("recurrence: amount and foreign amount swapped to the account's currency");
    Ok(TemplateSplitInput {
        foreign_amount: Some(split.amount.clone()),
        foreign_currency: split.currency.clone(),
        amount: foreign_amount,
        currency: Some(foreign_currency),
        ..split
    })
}

fn split(
    state: &State,
    lookups: &Lookups,
    t: &RecurrenceTransaction,
    report: &mut ImportReport,
) -> Result<TemplateSplitInput, String> {
    let account = |cash_ref: &str, id: &str| {
        state
            .refs
            .resolve(cash_ref, id)
            .or_else(|| state.refs.resolve(ACCOUNT_REF, id))
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
    let (foreign_amount, foreign_currency) = match (&t.foreign_amount, &t.foreign_currency_code) {
        (Some(amount), Some(code)) if code != &t.currency_code => {
            (Some(normalize(amount, places(code)?)?), Some(code.clone()))
        }
        _ => (None, None),
    };
    let mut optional = |reference: &str, id: &Option<String>, missing: &str| match id {
        Some(id) => {
            let found = state.refs.resolve(reference, id).map(str::to_string);
            if found.is_none() {
                report.note(missing);
            }
            found
        }
        None => None,
    };
    let category = optional(
        "category",
        &t.category_id,
        "recurrence: category not imported, link dropped",
    );
    let budget = optional(
        BUDGET_REF,
        &t.budget_id,
        "recurrence: budget not imported, link dropped",
    );
    let bill_firefly = t.subscription_id.clone().or_else(|| t.bill_id.clone());
    let bill = optional(
        BILL_REF,
        &bill_firefly,
        "recurrence: subscription not imported, link dropped",
    );
    let piggy = optional(
        PIGGY_REF,
        &t.piggy_bank_id,
        "recurrence: piggy bank not imported, link dropped",
    );
    let mut tag_ids = Vec::new();
    for name in t.tags.iter().flatten() {
        match lookups
            .tags
            .get(&tags::name_key(name))
            .and_then(|id| id.parse().ok())
        {
            // Firefly may name one Saldo tag in several cases; keep it once.
            Some(id) if tag_ids.contains(&id) => {}
            Some(id) => tag_ids.push(id),
            None => report.note("recurrence: tag not imported, dropped"),
        }
    }
    Ok(TemplateSplitInput {
        description: t.description.trim().to_string(),
        source_id: account(CASH_REVENUE, &t.source_id)?,
        destination_id: account(CASH_EXPENSE, &t.destination_id)?,
        amount: normalize(&t.amount, places(&t.currency_code)?)?,
        currency: Some(t.currency_code.clone()),
        foreign_amount,
        foreign_currency,
        category_id: parse_local(category),
        budget_id: parse_local(budget),
        bill_id: parse_local(bill),
        piggy_bank_id: parse_local(piggy),
        tag_ids,
    })
}

/// A local ID from a resolved reference. A function, not a closure: a
/// closure has one fixed type, while a generic function is compiled once per
/// ID type it is used with (like a Go generic function).
fn parse_local<T: std::str::FromStr>(text: Option<String>) -> Option<T> {
    text.and_then(|id| id.parse().ok())
}

/// Whether the stored recurrence already equals what the import would write.
fn same(view: &RecurrenceView, input: &RecurrenceInput) -> bool {
    let text = |d: Option<chrono::NaiveDate>| d.map(|d| d.to_string());
    let header = view.title == input.title
        && view.description == input.description
        && view.first_date == input.first_date.to_string()
        && view.repeat_until == text(input.repeat_until)
        && view.max_occurrences == input.max_occurrences.map(i64::from)
        && Some(view.apply_rules) == input.apply_rules
        && Some(view.active) == input.active
        && view.notes == input.notes;
    let repetitions = view.repetitions.len() == input.repetitions.len()
        && view
            .repetitions
            .iter()
            .zip(&input.repetitions)
            .all(|(v, i)| {
                // Compare the normalised moment (`2,3` and ` 2, 3` are the same).
                let moment = Pattern::parse(&i.kind, &i.moment)
                    .map(|p| p.moment())
                    .unwrap_or_default();
                v.kind == i.kind
                    && v.moment == moment
                    && v.skip == i64::from(i.skip)
                    && Some(v.weekend_policy.as_str()) == i.weekend_policy.as_deref()
            });
    let splits = view.splits.len() == input.splits.len()
        && view.splits.iter().zip(&input.splits).all(|(v, i)| {
            let tags: BTreeSet<String> = i.tag_ids.iter().map(ToString::to_string).collect();
            let stored: BTreeSet<String> = v.tag_ids.iter().cloned().collect();
            v.description == i.description
                && v.source_account_id == i.source_id.to_string()
                && v.destination_account_id == i.destination_id.to_string()
                && v.amount == i.amount
                && Some(v.currency.as_str()) == i.currency.as_deref()
                && v.foreign_amount == i.foreign_amount
                && v.foreign_currency == i.foreign_currency
                && v.category_id == i.category_id.map(|c| c.to_string())
                && v.budget_id == i.budget_id.map(|b| b.to_string())
                && v.bill_id == i.bill_id.map(|b| b.to_string())
                && v.piggy_bank_id == i.piggy_bank_id.map(|p| p.to_string())
                && stored == tags
        });
    header && repetitions && splits
}
