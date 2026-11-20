//! CSV files for Phase 10: saved views and net worth goals.

use super::ExportDocument;
use super::csv::{opt, table};

pub(crate) fn files(doc: &ExportDocument) -> Vec<(&'static str, String)> {
    vec![
        ("saved_views.csv", saved_views(doc)),
        ("net_worth_goals.csv", net_worth_goals(doc)),
    ]
}

/// One row per view; `filters` is the stored JSON object.
fn saved_views(doc: &ExportDocument) -> String {
    let header = [
        "id",
        "name",
        "page",
        "filters",
        "sort_order",
        "created_at",
        "updated_at",
    ];
    let rows = doc
        .saved_views
        .iter()
        .map(|v| {
            vec![
                v.id.clone(),
                v.name.clone(),
                v.page.clone(),
                v.filters.to_string(),
                v.sort_order.to_string(),
                v.created_at.clone(),
                v.updated_at.clone(),
            ]
        })
        .collect();
    table(&header, rows)
}

/// One row per goal, with its progress on the export day.
fn net_worth_goals(doc: &ExportDocument) -> String {
    let header = [
        "id",
        "name",
        "owner_user_id",
        "target_amount",
        "start_amount",
        "current",
        "currency",
        "target_date",
        "reached_on",
        "active",
        "notes",
        "created_at",
    ];
    let rows = doc
        .net_worth_goals
        .iter()
        .map(|g| {
            vec![
                g.id.clone(),
                g.name.clone(),
                opt(&g.owner_user_id),
                g.target_amount.clone(),
                g.start_amount.clone(),
                g.progress.current.clone(),
                g.currency.clone(),
                opt(&g.target_date),
                opt(&g.reached_on),
                g.active.to_string(),
                opt(&g.notes),
                g.created_at.clone(),
            ]
        })
        .collect();
    table(&header, rows)
}
