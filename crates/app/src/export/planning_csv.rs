//! CSV files for the planning entities (Phase 5): bills, their periods,
//! rule groups and rules.

use std::collections::HashMap;

use super::ExportDocument;
use super::csv::{opt, table};
use crate::rules::{ActionView, RuleView, TriggerView};

/// The planning CSV files of a document, by file name.
pub(crate) fn files(doc: &ExportDocument) -> Vec<(&'static str, String)> {
    vec![
        ("bills.csv", bills(doc)),
        ("bill_periods.csv", bill_periods(doc)),
        ("rule_groups.csv", rule_groups(doc)),
        ("rules.csv", rules(doc)),
    ]
}

fn bills(doc: &ExportDocument) -> String {
    let header = [
        "id",
        "name",
        "amount_min",
        "amount_max",
        "currency",
        "first_due_date",
        "end_date",
        "repeat_freq",
        "skip",
        "remind_days_before",
        "remind_every_days",
        "active",
        "notes",
    ];
    let rows = doc
        .bills
        .iter()
        .map(|b| {
            vec![
                b.id.clone(),
                b.name.clone(),
                b.amount_min.clone(),
                b.amount_max.clone(),
                b.currency.clone(),
                b.first_due_date.clone(),
                opt(&b.end_date),
                b.repeat_freq.clone(),
                b.skip.to_string(),
                b.remind_days_before.to_string(),
                b.remind_every_days.to_string(),
                b.active.to_string(),
                opt(&b.notes),
            ]
        })
        .collect();
    table(&header, rows)
}

fn bill_periods(doc: &ExportDocument) -> String {
    let bill_names = names(doc.bills.iter().map(|b| (b.id.as_str(), b.name.as_str())));
    let header = [
        "id",
        "bill",
        "due_date",
        "status",
        "paid_split_id",
        "paid_at",
        "last_reminded_on",
        "reminder_count",
    ];
    let rows = doc
        .bill_periods
        .iter()
        .map(|p| {
            vec![
                p.id.clone(),
                lookup(&bill_names, &p.bill_id),
                p.due_date.clone(),
                p.status.clone(),
                opt(&p.paid_split_id),
                opt(&p.paid_at),
                opt(&p.last_reminded_on),
                p.reminder_count.to_string(),
            ]
        })
        .collect();
    table(&header, rows)
}

fn rule_groups(doc: &ExportDocument) -> String {
    let header = ["id", "title", "description", "sort_order", "active"];
    let rows = doc
        .rule_groups
        .iter()
        .map(|g| {
            vec![
                g.id.clone(),
                g.title.clone(),
                opt(&g.description),
                g.sort_order.to_string(),
                g.active.to_string(),
            ]
        })
        .collect();
    table(&header, rows)
}

fn rules(doc: &ExportDocument) -> String {
    let group_titles = names(
        doc.rule_groups
            .iter()
            .map(|g| (g.id.as_str(), g.title.as_str())),
    );
    let header = [
        "id",
        "group",
        "title",
        "trigger_on",
        "strict",
        "stop_processing",
        "active",
        "sort_order",
        "triggers",
        "actions",
    ];
    let rows = doc
        .rules
        .iter()
        .map(|r| rule_row(r, &group_titles))
        .collect();
    table(&header, rows)
}

fn rule_row(rule: &RuleView, group_titles: &HashMap<&str, &str>) -> Vec<String> {
    let triggers: Vec<String> = rule.triggers.iter().map(trigger_text).collect();
    let actions: Vec<String> = rule.actions.iter().map(action_text).collect();
    vec![
        rule.id.clone(),
        lookup(group_titles, &rule.rule_group_id),
        rule.title.clone(),
        rule.trigger_on.clone(),
        rule.strict.to_string(),
        rule.stop_processing.to_string(),
        rule.active.to_string(),
        rule.sort_order.to_string(),
        triggers.join("; "),
        actions.join("; "),
    ]
}

/// `description_contains=Shop`, with `!` in front when negated.
fn trigger_text(trigger: &TriggerView) -> String {
    let not = if trigger.negate { "!" } else { "" };
    format!("{not}{}", keyword(&trigger.kind, &trigger.value))
}

/// `set_notes=weekly shop`, with ` (inactive)` after a disabled action.
fn action_text(action: &ActionView) -> String {
    let off = if action.active { "" } else { " (inactive)" };
    format!("{}{off}", keyword(&action.kind, &action.value))
}

fn keyword(kind: &str, value: &Option<String>) -> String {
    match value {
        Some(value) => format!("{kind}={value}"),
        None => kind.to_string(),
    }
}

fn names<'a>(pairs: impl Iterator<Item = (&'a str, &'a str)>) -> HashMap<&'a str, &'a str> {
    pairs.collect()
}

fn lookup(map: &HashMap<&str, &str>, id: &str) -> String {
    map.get(id).copied().unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_parts_read_as_keywords() {
        let trigger = TriggerView {
            kind: "amount_more".into(),
            value: Some("10".into()),
            negate: true,
        };
        let action = ActionView {
            kind: "remove_all_tags".into(),
            value: None,
            stop_processing: false,
            active: false,
        };
        assert_eq!(trigger_text(&trigger), "!amount_more=10");
        assert_eq!(action_text(&action), "remove_all_tags (inactive)");
    }
}
