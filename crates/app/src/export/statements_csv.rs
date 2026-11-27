//! CSV files for Phase 13: statement profiles, and the Amazon and PayPal
//! exports with their rows and booked matches.

use super::ExportDocument;
use super::csv::table;

pub(crate) fn files(doc: &ExportDocument) -> Vec<(&'static str, String)> {
    vec![
        ("statement_profiles.csv", profiles(doc)),
        ("source_documents.csv", documents(doc)),
        ("source_rows.csv", rows(doc)),
        ("source_matches.csv", matches(doc)),
    ]
}

fn profiles(doc: &ExportDocument) -> String {
    let header = [
        "id",
        "name",
        "account_id",
        "format",
        "mapping",
        "part_accounts",
        "transfer_rules",
        "created_at",
        "updated_at",
    ];
    let rows = doc
        .statement_profiles
        .iter()
        .map(|p| {
            vec![
                p.id.clone(),
                p.name.clone(),
                p.account_id.clone().unwrap_or_default(),
                p.format.clone(),
                p.mapping.to_string(),
                p.part_accounts.to_string(),
                serde_json::to_string(&p.transfer_rules).unwrap_or_default(),
                p.created_at.clone(),
                p.updated_at.clone(),
            ]
        })
        .collect();
    table(&header, rows)
}

fn documents(doc: &ExportDocument) -> String {
    let header = [
        "id",
        "kind",
        "filename",
        "first_date",
        "last_date",
        "row_count",
        "rows_added",
        "rows_updated",
        "created_at",
    ];
    let rows = doc
        .source_documents
        .iter()
        .map(|d| {
            vec![
                d.id.clone(),
                d.kind.clone(),
                d.filename.clone(),
                d.first_date.clone().unwrap_or_default(),
                d.last_date.clone().unwrap_or_default(),
                d.row_count.to_string(),
                d.rows_added.to_string(),
                d.rows_updated.to_string(),
                d.created_at.clone(),
            ]
        })
        .collect();
    table(&header, rows)
}

fn rows(doc: &ExportDocument) -> String {
    let header = [
        "id",
        "kind",
        "external_id",
        "date",
        "time",
        "amount",
        "currency",
        "row_kind",
        "group_key",
        "counterparty",
        "text",
        "status",
        "excluded",
    ];
    let rows = doc
        .source_rows
        .iter()
        .map(|r| {
            vec![
                r.id.clone(),
                r.kind.clone(),
                r.external_id.clone(),
                r.date.clone(),
                r.time.clone().unwrap_or_default(),
                r.amount.clone(),
                r.currency.clone(),
                r.row_kind.clone(),
                r.group_key.clone().unwrap_or_default(),
                r.counterparty.clone().unwrap_or_default(),
                r.text.clone().unwrap_or_default(),
                r.status.clone().unwrap_or_default(),
                r.excluded.to_string(),
            ]
        })
        .collect();
    table(&header, rows)
}

fn matches(doc: &ExportDocument) -> String {
    let header = [
        "source_row_id",
        "split_id",
        "match_type",
        "amount_diff_minor",
        "created_at",
    ];
    let rows = doc
        .source_matches
        .iter()
        .map(|m| {
            vec![
                m.source_row_id.clone(),
                m.split_id.clone(),
                m.match_type.clone(),
                m.amount_diff_minor.to_string(),
                m.created_at.clone(),
            ]
        })
        .collect();
    table(&header, rows)
}
