//! `/v1/rule-groups` and `/v1/rules` → rule groups and rules (ARCHITECTURE.md
//! 9.1, row 7). Firefly rules refer to objects by name; a rule with a keyword
//! Saldo does not support, or a name it cannot find, is imported inactive with
//! that keyword kept as written, and listed in the report.

use saldo_app::ctx::Origin;
use saldo_app::rules::{
    ActionView, NewRuleGroup, RuleGroupUpdate, RuleInput, RuleNames, RuleView, TriggerView,
    names_in,
};
use saldo_core::{Action, Trigger};

use crate::amounts::normalize;
use crate::engine::{ImportCx, ImportError, State};
use crate::firefly::models::{Resource, Rule, RuleGroup, RulePart};
use crate::report::ImportReport;

const GROUPS: &str = "rule_groups";
const RULES: &str = "rules";
const GROUP_REF: &str = "rule_group";
const RULE_REF: &str = "rule";

pub async fn import_rules(cx: &ImportCx, report: &mut ImportReport) -> Result<(), ImportError> {
    let groups = cx.firefly.list::<RuleGroup>("/v1/rule-groups", &[]).await?;
    let rules = cx.firefly.list::<Rule>("/v1/rules", &[]).await?;
    let mut guard = cx.state.lock().await;
    let state = &mut *guard;
    for group in &groups {
        import_group(cx, state, report, group).await?;
    }
    let names = names_in(&mut state.tx, &cx.ctx).await?;
    for rule in &rules {
        import_rule(cx, state, report, &names, rule).await?;
    }
    Ok(())
}

async fn import_group(
    cx: &ImportCx,
    state: &mut State,
    report: &mut ImportReport,
    item: &Resource<RuleGroup>,
) -> Result<(), ImportError> {
    let g = &item.attributes;
    let wanted = NewRuleGroup {
        title: g.title.trim().to_string(),
        description: g.description.clone().filter(|d| !d.trim().is_empty()),
        sort_order: g.order.unwrap_or(0),
        active: g.active,
    };
    let service = &cx.app.rules;
    let current = match state.refs.resolve(GROUP_REF, &item.id).map(str::to_string) {
        Some(id) => service.get_group_in(&mut state.tx, &cx.ctx, &id).await.ok(),
        None => None,
    };
    let local = match current {
        Some(view)
            if view.title == wanted.title
                && view.description == wanted.description
                && view.sort_order == wanted.sort_order
                && view.active == wanted.active =>
        {
            report.unchanged(GROUPS);
            return Ok(());
        }
        Some(view) => {
            let update = RuleGroupUpdate {
                title: Some(wanted.title),
                description: Some(wanted.description),
                sort_order: Some(wanted.sort_order),
                active: Some(wanted.active),
            };
            report.updated(GROUPS);
            service
                .update_group_in(&mut state.tx, &cx.ctx, Origin::Import, &view.id, update)
                .await?
                .id
        }
        None => {
            report.created(GROUPS);
            service
                .create_group_in(&mut state.tx, &cx.ctx, Origin::Import, wanted)
                .await?
                .id
        }
    };
    state
        .refs
        .upsert(&mut state.tx, GROUP_REF, &item.id, &local, &cx.now())
        .await?;
    Ok(())
}

async fn import_rule(
    cx: &ImportCx,
    state: &mut State,
    report: &mut ImportReport,
    names: &RuleNames,
    item: &Resource<Rule>,
) -> Result<(), ImportError> {
    let r = &item.attributes;
    let Some(group) = state
        .refs
        .resolve(GROUP_REF, &r.rule_group_id)
        .map(str::to_string)
    else {
        report.skipped(
            RULES,
            format!("{:?}: its rule group was not imported", r.title),
        );
        return Ok(());
    };
    let mut problems = Vec::new();
    let triggers = sorted(&r.triggers)
        .into_iter()
        .map(|t| trigger(t, names, &mut problems))
        .collect();
    let actions = sorted(&r.actions)
        .into_iter()
        .map(|a| action(a, names, &mut problems))
        .collect();
    let wanted = RuleInput {
        rule_group_id: group,
        title: r.title.trim().to_string(),
        description: r.description.clone().filter(|d| !d.trim().is_empty()),
        trigger_on: trigger_on(&r.trigger).to_string(),
        strict: r.strict,
        stop_processing: r.stop_processing,
        active: r.active && problems.is_empty(),
        sort_order: r.order.unwrap_or(0),
        triggers,
        actions,
    };
    if r.active && !problems.is_empty() {
        report.message(
            RULES,
            format!("{:?} imported inactive: {}", r.title, problems.join("; ")),
        );
        report.note("rule imported inactive (unsupported keyword or unknown name)");
    }
    let service = &cx.app.rules;
    let current = match state.refs.resolve(RULE_REF, &item.id).map(str::to_string) {
        Some(id) => service.get_rule_in(&mut state.tx, &cx.ctx, &id).await.ok(),
        None => None,
    };
    let result = match current {
        Some(view) if same(&view, &wanted) => {
            report.unchanged(RULES);
            return Ok(());
        }
        Some(view) => service
            .replace_rule_in(&mut state.tx, &cx.ctx, Origin::Import, &view.id, wanted)
            .await
            .map(|v| (v.id, false)),
        None => service
            .create_rule_in(&mut state.tx, &cx.ctx, Origin::Import, wanted)
            .await
            .map(|v| (v.id, true)),
    };
    match result {
        Ok((local, created)) => {
            state
                .refs
                .upsert(&mut state.tx, RULE_REF, &item.id, &local, &cx.now())
                .await?;
            if created {
                report.created(RULES)
            } else {
                report.updated(RULES)
            }
        }
        Err(error) => report.skipped(
            RULES,
            format!("{:?}: {}", r.title, super::skippable(error)?),
        ),
    }
    Ok(())
}

fn sorted(parts: &[RulePart]) -> Vec<&RulePart> {
    let mut parts: Vec<&RulePart> = parts.iter().collect();
    parts.sort_by_key(|p| p.order.unwrap_or(0));
    parts
}

fn trigger_on(firefly: &str) -> &'static str {
    match firefly {
        "store-journal" => "create",
        "update-journal" => "update",
        _ => "manual",
    }
}

/// Firefly's trigger as Saldo's: names become IDs, amounts are normalised.
/// Anything Saldo cannot express is kept as written and noted in `problems`.
fn trigger(part: &RulePart, names: &RuleNames, problems: &mut Vec<String>) -> TriggerView {
    let kind = match part.kind.as_str() {
        "notes_contain" => "notes_contains".to_string(),
        other => other.to_string(),
    };
    let raw = part.value.clone().unwrap_or_default();
    let value = match kind.as_str() {
        "amount_less" | "amount_more" | "amount_exactly" => {
            normalize(&raw, 8).ok().map(|v| trim_zeros(&v))
        }
        "currency_is" => currency(&raw, names),
        "category_is" => names.categories.get(&raw.trim().to_lowercase()).cloned(),
        "tag_is" => names.tags.get(&raw.trim().to_lowercase()).cloned(),
        "source_account_is" | "destination_account_is" => {
            names.accounts.get(&raw.trim().to_lowercase()).cloned()
        }
        _ => Some(raw.clone()),
    };
    let negate = part.prohibited;
    let parsed = value.as_deref().map(|v| Trigger::parse(&kind, Some(v)));
    match (value, parsed) {
        (Some(_), Some(Ok(trigger))) => TriggerView {
            kind,
            value: trigger.value(),
            negate,
        },
        _ => {
            problems.push(format!("trigger {kind} {raw:?}"));
            TriggerView {
                kind: part.kind.clone(),
                value: part.value.clone(),
                negate,
            }
        }
    }
}

fn action(part: &RulePart, names: &RuleNames, problems: &mut Vec<String>) -> ActionView {
    let raw = part.value.clone().unwrap_or_default();
    let key = raw.trim().to_lowercase();
    let value = match part.kind.as_str() {
        "set_category" => names.categories.get(&key).cloned(),
        "add_tag" | "remove_tag" => names.tags.get(&key).cloned(),
        "link_to_bill" => names.bills.get(&key).cloned(),
        "set_budget" => names.budgets.get(&key).cloned(),
        _ => Some(raw.clone()),
    };
    let parsed = value.as_deref().map(|v| Action::parse(&part.kind, Some(v)));
    match parsed {
        Some(Ok(action)) => ActionView {
            kind: part.kind.clone(),
            value: action.value(),
            stop_processing: part.stop_processing,
            active: part.active,
        },
        _ => {
            problems.push(format!("action {} {raw:?}", part.kind));
            ActionView {
                kind: part.kind.clone(),
                value: part.value.clone(),
                stop_processing: part.stop_processing,
                active: part.active,
            }
        }
    }
}

/// `"Euro (EUR)"`, `"EUR"` or `"Euro"` → `"EUR"`.
fn currency(raw: &str, names: &RuleNames) -> Option<String> {
    let text = raw.trim();
    let in_brackets = text
        .rsplit_once('(')
        .and_then(|(_, rest)| rest.strip_suffix(')'));
    let candidate = in_brackets.unwrap_or(text).trim().to_lowercase();
    names.currencies.get(&candidate).cloned()
}

/// `"700.00000000"` → `"700"`, `"12.50000000"` → `"12.5"`.
fn trim_zeros(text: &str) -> String {
    if !text.contains('.') {
        return text.to_string();
    }
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn same(view: &RuleView, wanted: &RuleInput) -> bool {
    view.rule_group_id == wanted.rule_group_id
        && view.title == wanted.title
        && view.description == wanted.description
        && view.trigger_on == wanted.trigger_on
        && view.strict == wanted.strict
        && view.stop_processing == wanted.stop_processing
        && view.active == wanted.active
        && view.sort_order == wanted.sort_order
        && view.triggers == wanted.triggers
        && view.actions == wanted.actions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> RuleNames {
        let mut names = RuleNames::default();
        names.currencies.insert("euro".into(), "EUR".into());
        names.currencies.insert("eur".into(), "EUR".into());
        names
    }

    #[test]
    fn maps_trigger_moments_currencies_and_amounts() {
        assert_eq!(trigger_on("store-journal"), "create");
        assert_eq!(trigger_on("update-journal"), "update");
        assert_eq!(trigger_on("manual-activation"), "manual");
        for raw in ["Euro (EUR)", "EUR", "euro"] {
            assert_eq!(currency(raw, &names()).as_deref(), Some("EUR"), "{raw}");
        }
        assert_eq!(currency("Dogecoin", &names()), None);
        assert_eq!(trim_zeros("700.00000000"), "700");
        assert_eq!(trim_zeros("12.50000000"), "12.5");
    }

    #[test]
    fn unknown_keywords_and_names_are_kept_and_reported() {
        let mut problems = Vec::new();
        let part = |kind: &str, value: &str| RulePart {
            kind: kind.into(),
            value: Some(value.into()),
            prohibited: false,
            stop_processing: false,
            active: true,
            order: None,
        };
        let kept = trigger(&part("journal_id", "12"), &names(), &mut problems);
        assert_eq!(
            (kept.kind.as_str(), kept.value.as_deref()),
            ("journal_id", Some("12"))
        );
        let missing = action(&part("set_category", "Nowhere"), &names(), &mut problems);
        assert_eq!(missing.value.as_deref(), Some("Nowhere"));
        assert_eq!(problems.len(), 2);
        let ok = trigger(
            &part("amount_more", "700.000000000000"),
            &names(),
            &mut problems,
        );
        assert_eq!(ok.value.as_deref(), Some("700"));
        let none = trigger(&part("has_no_tag", ""), &names(), &mut problems);
        assert_eq!(none.value, None);
        assert_eq!(problems.len(), 2);
    }
}
