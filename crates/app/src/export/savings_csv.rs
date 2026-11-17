//! CSV files for budgets, their limits, piggy banks and their events.

use super::ExportDocument;
use super::csv::{opt, table};
use super::planning_csv::{lookup, names};
use crate::budgets::AutoBudgetView;

/// The budget and piggy bank CSV files of a document, by file name.
pub(crate) fn files(doc: &ExportDocument) -> Vec<(&'static str, String)> {
    vec![
        ("budgets.csv", budgets(doc)),
        ("budget_limits.csv", budget_limits(doc)),
        ("piggy_banks.csv", piggy_banks(doc)),
        ("piggy_bank_events.csv", piggy_bank_events(doc)),
    ]
}

fn budgets(doc: &ExportDocument) -> String {
    let header = [
        "id",
        "name",
        "active",
        "auto_type",
        "auto_period",
        "auto_amount",
        "auto_currency",
        "carry",
        "notes",
    ];
    let rows = doc
        .budgets
        .iter()
        .map(|b| {
            let auto = b.auto_budget.as_ref();
            let field =
                |f: fn(&AutoBudgetView) -> &String| auto.map(f).cloned().unwrap_or_default();
            vec![
                b.id.clone(),
                b.name.clone(),
                b.active.to_string(),
                field(|a| &a.auto_type),
                field(|a| &a.period),
                field(|a| &a.amount),
                field(|a| &a.currency),
                b.carry.clone(),
                opt(&b.notes),
            ]
        })
        .collect();
    table(&header, rows)
}

fn budget_limits(doc: &ExportDocument) -> String {
    let budget_names = names(doc.budgets.iter().map(|b| (b.id.as_str(), b.name.as_str())));
    let header = [
        "id",
        "budget",
        "start_date",
        "end_date",
        "amount",
        "currency",
        "carried",
        "spent",
        "available",
        "notes",
    ];
    let rows = doc
        .budget_limits
        .iter()
        .map(|l| {
            vec![
                l.id.clone(),
                lookup(&budget_names, &l.budget_id),
                l.start_date.clone(),
                l.end_date.clone(),
                l.amount.clone(),
                l.currency.clone(),
                l.carried.clone(),
                l.spent.clone(),
                l.available.clone(),
                opt(&l.notes),
            ]
        })
        .collect();
    table(&header, rows)
}

fn piggy_banks(doc: &ExportDocument) -> String {
    let account_names = names(
        doc.accounts
            .iter()
            .map(|a| (a.id.as_str(), a.name.as_str())),
    );
    let header = [
        "id",
        "name",
        "currency",
        "target_amount",
        "saved_amount",
        "start_date",
        "target_date",
        "active",
        "accounts",
        "notes",
    ];
    let rows = doc
        .piggy_banks
        .iter()
        .map(|p| {
            // `Checking=25.00; Savings=10.00`: each linked account and what it holds.
            let accounts: Vec<String> = p
                .accounts
                .iter()
                .map(|a| {
                    format!(
                        "{}={}",
                        lookup(&account_names, &a.account_id),
                        a.saved_amount
                    )
                })
                .collect();
            vec![
                p.id.clone(),
                p.name.clone(),
                p.currency.clone(),
                opt(&p.target_amount),
                p.saved_amount.clone(),
                opt(&p.start_date),
                opt(&p.target_date),
                p.active.to_string(),
                accounts.join("; "),
                opt(&p.notes),
            ]
        })
        .collect();
    table(&header, rows)
}

fn piggy_bank_events(doc: &ExportDocument) -> String {
    let bank_names = names(
        doc.piggy_banks
            .iter()
            .map(|p| (p.id.as_str(), p.name.as_str())),
    );
    let account_names = names(
        doc.accounts
            .iter()
            .map(|a| (a.id.as_str(), a.name.as_str())),
    );
    let header = [
        "id",
        "piggy_bank",
        "account",
        "amount",
        "currency",
        "date",
        "split_id",
    ];
    let rows = doc
        .piggy_bank_events
        .iter()
        .map(|e| {
            vec![
                e.id.clone(),
                lookup(&bank_names, &e.piggy_bank_id),
                lookup(&account_names, &e.account_id),
                e.amount.clone(),
                e.currency.clone(),
                e.date.clone(),
                opt(&e.split_id),
            ]
        })
        .collect();
    table(&header, rows)
}
