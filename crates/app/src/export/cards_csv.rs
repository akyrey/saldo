//! CSV files for Phase 12: credit card plans, their budgets and moves.

use super::ExportDocument;
use super::csv::table;

pub(crate) fn files(doc: &ExportDocument) -> Vec<(&'static str, String)> {
    vec![
        ("credit_card_plans.csv", plans(doc)),
        ("credit_card_limits.csv", limits(doc)),
        ("credit_card_moves.csv", moves(doc)),
    ]
}

fn plans(doc: &ExportDocument) -> String {
    let header = [
        "account_id",
        "currency",
        "settlement_account_id",
        "settlement_day",
        "starts_cycle",
        "created_at",
        "updated_at",
    ];
    let rows = doc
        .credit_card_plans
        .iter()
        .map(|p| {
            vec![
                p.account_id.clone(),
                p.currency.clone(),
                p.settlement_account_id.clone(),
                p.settlement_day.to_string(),
                p.starts_cycle.clone(),
                p.created_at.clone(),
                p.updated_at.clone(),
            ]
        })
        .collect();
    table(&header, rows)
}

/// One row per budget: it applies from `from_cycle` until the next one.
fn limits(doc: &ExportDocument) -> String {
    let header = ["account_id", "from_cycle", "amount", "currency"];
    let rows = doc
        .credit_card_plans
        .iter()
        .flat_map(|p| {
            p.limits.iter().map(|l| {
                vec![
                    p.account_id.clone(),
                    l.from_cycle.clone(),
                    l.amount.clone(),
                    p.currency.clone(),
                ]
            })
        })
        .collect();
    table(&header, rows)
}

fn moves(doc: &ExportDocument) -> String {
    let header = ["split_id", "account_id", "created_at"];
    let rows = doc
        .credit_card_moves
        .iter()
        .map(|m| {
            vec![
                m.split_id.clone(),
                m.account_id.clone(),
                m.created_at.clone(),
            ]
        })
        .collect();
    table(&header, rows)
}
