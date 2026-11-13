//! Running rules on a saved transaction group.

use std::collections::{HashMap, HashSet};

use saldo_core::rules::{apply, evaluate, matches};
use saldo_core::{CurrencyCode, Rule, RuleAction, RuleSplit, RuleTrigger, SplitKind, Trigger};
use sqlx::SqliteConnection;

use super::store::{self, RuleView};
use crate::ctx::HouseholdCtx;
use crate::error::{AppError, parse_stored};
use crate::transactions::{SplitView, TransactionGroupView, TransactionInput};

/// A rule ready to evaluate.
pub(crate) struct LoadedRule {
    pub id: String,
    pub group_id: String,
    pub rule: Rule,
    pub stop_processing: bool,
}

/// Turns a stored rule into a core `Rule`. `Err` for keywords Saldo does not
/// support (possible on inactive, imported rules).
pub(crate) fn to_rule(view: &RuleView) -> Result<LoadedRule, String> {
    let triggers = view
        .triggers
        .iter()
        .map(|t| {
            Trigger::parse(&t.kind, t.value.as_deref()).map(|trigger| RuleTrigger {
                trigger,
                negate: t.negate,
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let actions = view
        .actions
        .iter()
        .map(|a| {
            saldo_core::Action::parse(&a.kind, a.value.as_deref()).map(|action| RuleAction {
                action,
                stop_processing: a.stop_processing,
                active: a.active,
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(LoadedRule {
        id: view.id.clone(),
        group_id: view.rule_group_id.clone(),
        rule: Rule {
            triggers,
            actions,
            strict: view.strict,
        },
        stop_processing: view.stop_processing,
    })
}

/// The active rules of active groups that run on `trigger_on`, in order.
pub(crate) async fn load(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    trigger_on: &str,
) -> Result<Vec<LoadedRule>, AppError> {
    let household_id = ctx.household_id.to_string();
    let active_groups: HashSet<String> = store::groups(&mut *conn, &household_id)
        .await?
        .into_iter()
        .filter(|g| g.active)
        .map(|g| g.id)
        .collect();
    let mut loaded = Vec::new();
    for view in store::rules(&mut *conn, &household_id, None).await? {
        if !view.active
            || view.trigger_on != trigger_on
            || !active_groups.contains(&view.rule_group_id)
        {
            continue;
        }
        match to_rule(&view) {
            Ok(rule) => loaded.push(rule),
            Err(error) => {
                tracing::warn!(rule = %view.id, %error, "active rule cannot be read; skipped")
            }
        }
    }
    Ok(loaded)
}

pub(crate) async fn decimal_places(
    conn: &mut SqliteConnection,
) -> Result<HashMap<String, u8>, AppError> {
    Ok(crate::currencies::all_in(conn)
        .await?
        .into_iter()
        .map(|c| (c.code, c.decimal_places))
        .collect())
}

/// What a rule sees of a stored split.
pub(crate) fn rule_split(
    split: &SplitView,
    places: &HashMap<String, u8>,
) -> Result<RuleSplit, AppError> {
    let corrupt = |what: &str| AppError::Internal(format!("bad {what} on split {}", split.id));
    let decimal_places = *places
        .get(&split.currency)
        .ok_or_else(|| corrupt("currency"))?;
    let currency = CurrencyCode::new(&split.currency).map_err(|_| corrupt("currency"))?;
    let amount = saldo_core::Money::parse_decimal(
        &split.amount,
        &saldo_core::Currency::new(currency, decimal_places).map_err(|_| corrupt("currency"))?,
    )
    .map_err(|_| corrupt("amount"))?;
    let kind: SplitKind = serde_json::from_value(serde_json::Value::String(split.kind.clone()))
        .map_err(|_| corrupt("kind"))?;
    Ok(RuleSplit {
        description: split.description.clone(),
        notes: split.notes.clone(),
        amount_minor: amount.minor(),
        currency,
        decimal_places,
        kind,
        source: parse_stored(&split.source_account_id)?,
        destination: parse_stored(&split.destination_account_id)?,
        category: split.category_id.as_deref().map(parse_stored).transpose()?,
        budget: split.budget_id.as_deref().map(parse_stored).transpose()?,
        bill: split.bill_id.as_deref().map(parse_stored).transpose()?,
        tags: split
            .tag_ids
            .iter()
            .map(|t| parse_stored(t))
            .collect::<Result<_, _>>()?,
    })
}

/// Runs `rules` in order on one split. A matching rule with
/// `stop_processing` skips the later rules of its group.
pub(crate) fn run(rules: &[LoadedRule], split: &RuleSplit) -> (RuleSplit, Vec<String>) {
    let mut current = split.clone();
    let mut matched = Vec::new();
    let mut stopped: HashSet<&str> = HashSet::new();
    for loaded in rules {
        if stopped.contains(loaded.group_id.as_str()) || !matches(&loaded.rule, &current) {
            continue;
        }
        current = apply(&current, &evaluate(&loaded.rule, &current));
        matched.push(loaded.id.clone());
        if loaded.stop_processing {
            stopped.insert(&loaded.group_id);
        }
    }
    (current, matched)
}

/// The group with every split after `rules`, or `None` if nothing changed.
pub(crate) async fn changes(
    conn: &mut SqliteConnection,
    rules: &[LoadedRule],
    view: &TransactionGroupView,
) -> Result<Option<TransactionInput>, AppError> {
    if rules.is_empty() {
        return Ok(None);
    }
    let places = decimal_places(conn).await?;
    let mut input = crate::transactions::input_from_view(view)?;
    let mut changed = false;
    for (split, target) in view.splits.iter().zip(input.splits.iter_mut()) {
        let before = rule_split(split, &places)?;
        let (after, _) = run(rules, &before);
        if after == before {
            continue;
        }
        changed = true;
        target.description = after.description;
        target.notes = after.notes;
        target.category_id = after.category;
        target.budget_id = after.budget;
        target.bill_id = after.bill;
        target.tag_ids = after.tags;
    }
    Ok(changed.then_some(input))
}
