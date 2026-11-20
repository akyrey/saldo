//! CSV files for Phase 10: saved views.

use super::ExportDocument;
use super::csv::table;

pub(crate) fn files(doc: &ExportDocument) -> Vec<(&'static str, String)> {
    vec![("saved_views.csv", saved_views(doc))]
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
