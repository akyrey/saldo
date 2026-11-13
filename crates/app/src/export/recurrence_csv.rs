//! `recurrences.csv`: one row per template split, like `transactions.csv`,
//! with the schedule repeated on each row and names instead of IDs.

use std::collections::HashMap;

use super::ExportDocument;
use super::csv::{opt, table};
use super::planning_csv::{lookup, names};
use crate::recurrences::{RecurrenceView, RepetitionView, TemplateSplitView};

const HEADER: [&str; 18] = [
    "id",
    "title",
    "first_date",
    "repeat_until",
    "max_occurrences",
    "repetitions",
    "active",
    "apply_rules",
    "description",
    "source_account",
    "destination_account",
    "amount",
    "currency",
    "category",
    "budget",
    "bill",
    "piggy_bank",
    "tags",
];

/// Names of everything a template split refers to, by ID.
struct Names<'a> {
    accounts: HashMap<&'a str, &'a str>,
    categories: HashMap<&'a str, &'a str>,
    budgets: HashMap<&'a str, &'a str>,
    bills: HashMap<&'a str, &'a str>,
    piggy_banks: HashMap<&'a str, &'a str>,
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
        piggy_banks: names(
            doc.piggy_banks
                .iter()
                .map(|p| (p.id.as_str(), p.name.as_str())),
        ),
        tags: names(doc.tags.iter().map(|t| (t.id.as_str(), t.name.as_str()))),
    };
    let rows = doc
        .recurrences
        .iter()
        .flat_map(|r| r.splits.iter().map(move |s| (r, s)))
        .map(|(r, s)| row(r, s, &names))
        .collect();
    vec![("recurrences.csv", table(&HEADER, rows))]
}

fn row(r: &RecurrenceView, s: &TemplateSplitView, names: &Names) -> Vec<String> {
    let name = |map: &HashMap<&str, &str>, id: &Option<String>| {
        id.as_deref().map(|id| lookup(map, id)).unwrap_or_default()
    };
    let repetitions: Vec<String> = r.repetitions.iter().map(repetition_text).collect();
    let tags: Vec<String> = s.tag_ids.iter().map(|t| lookup(&names.tags, t)).collect();
    vec![
        r.id.clone(),
        r.title.clone(),
        r.first_date.clone(),
        opt(&r.repeat_until),
        r.max_occurrences.map(|m| m.to_string()).unwrap_or_default(),
        repetitions.join("; "),
        r.active.to_string(),
        r.apply_rules.to_string(),
        s.description.clone(),
        lookup(&names.accounts, &s.source_account_id),
        lookup(&names.accounts, &s.destination_account_id),
        s.amount.clone(),
        s.currency.clone(),
        name(&names.categories, &s.category_id),
        name(&names.budgets, &s.budget_id),
        name(&names.bills, &s.bill_id),
        name(&names.piggy_banks, &s.piggy_bank_id),
        tags.join("; "),
    ]
}

/// `monthly 1`, `weekly 5 every 2 (next_monday)`: kind and moment, then the
/// skip and weekend policy only when they are not the defaults.
fn repetition_text(r: &RepetitionView) -> String {
    let mut text = format!("{} {}", r.kind, r.moment).trim_end().to_string();
    if r.skip > 0 {
        text = format!("{text} every {}", r.skip + 1);
    }
    if r.weekend_policy != "keep" {
        text = format!("{text} ({})", r.weekend_policy);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repetitions_read_as_short_phrases() {
        let rep = |kind: &str, moment: &str, skip, policy: &str| RepetitionView {
            kind: kind.into(),
            moment: moment.into(),
            skip,
            weekend_policy: policy.into(),
        };
        assert_eq!(
            repetition_text(&rep("monthly", "1", 0, "keep")),
            "monthly 1"
        );
        assert_eq!(repetition_text(&rep("daily", "", 0, "keep")), "daily");
        assert_eq!(
            repetition_text(&rep("weekly", "5", 1, "next_monday")),
            "weekly 5 every 2 (next_monday)"
        );
    }
}
