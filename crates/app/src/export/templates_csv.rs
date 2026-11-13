//! `templates.csv`: one row per transaction template split, with names
//! instead of IDs. A template without splits still gets one row.

use std::collections::HashMap;

use super::ExportDocument;
use super::csv::{opt, table};
use super::planning_csv::{lookup, names};
use crate::templates::{TransactionTemplateSplitView, TransactionTemplateView};

const HEADER: [&str; 17] = [
    "id",
    "name",
    "kind",
    "title",
    "sort_order",
    "description",
    "source_account",
    "destination_account",
    "amount",
    "currency",
    "foreign_amount",
    "foreign_currency",
    "category",
    "budget",
    "bill",
    "tags",
    "notes",
];

/// Names of everything a template split refers to, by ID.
struct Names<'a> {
    accounts: HashMap<&'a str, &'a str>,
    categories: HashMap<&'a str, &'a str>,
    budgets: HashMap<&'a str, &'a str>,
    bills: HashMap<&'a str, &'a str>,
    tags: HashMap<&'a str, &'a str>,
}

pub(crate) fn files(doc: &ExportDocument) -> Vec<(&'static str, String)> {
    let names = Names {
        accounts: names(
            doc.accounts
                .iter()
                .map(|a| (a.id.as_str(), a.name.as_str())),
        ),
        categories: names(
            doc.categories
                .iter()
                .map(|c| (c.id.as_str(), c.name.as_str())),
        ),
        budgets: names(doc.budgets.iter().map(|b| (b.id.as_str(), b.name.as_str()))),
        bills: names(doc.bills.iter().map(|b| (b.id.as_str(), b.name.as_str()))),
        tags: names(doc.tags.iter().map(|t| (t.id.as_str(), t.name.as_str()))),
    };
    let empty = TransactionTemplateSplitView::default();
    let rows = doc
        .templates
        .iter()
        .flat_map(|t| {
            let splits: Vec<&TransactionTemplateSplitView> = if t.splits.is_empty() {
                vec![&empty]
            } else {
                t.splits.iter().collect()
            };
            splits.into_iter().map(move |s| (t, s))
        })
        .map(|(t, s)| row(t, s, &names))
        .collect();
    vec![("templates.csv", table(&HEADER, rows))]
}

fn row(
    t: &TransactionTemplateView,
    s: &TransactionTemplateSplitView,
    names: &Names,
) -> Vec<String> {
    let name = |map: &HashMap<&str, &str>, id: &Option<String>| {
        id.as_deref().map(|id| lookup(map, id)).unwrap_or_default()
    };
    let tags: Vec<String> = s.tag_ids.iter().map(|t| lookup(&names.tags, t)).collect();
    vec![
        t.id.clone(),
        t.name.clone(),
        t.kind.clone(),
        opt(&t.title),
        t.sort_order.to_string(),
        opt(&s.description),
        name(&names.accounts, &s.source_id),
        name(&names.accounts, &s.destination_id),
        opt(&s.amount),
        opt(&s.currency),
        opt(&s.foreign_amount),
        opt(&s.foreign_currency),
        name(&names.categories, &s.category_id),
        name(&names.budgets, &s.budget_id),
        name(&names.bills, &s.bill_id),
        tags.join("; "),
        opt(&s.notes),
    ]
}
