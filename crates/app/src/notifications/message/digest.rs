//! The weekly digest: one plain-text summary of the previous week.

use std::fmt::Write;

use serde_json::Value;

use super::Message;

/// `data`: `{ week: { from, to }, currencies: [{ currency, income, expenses }],
/// top_categories: [{ name, expenses, currency }], bills_due: [{ name,
/// due_date, amount, currency }], budgets: [{ name, percent }] }`.
pub(super) fn weekly(data: &Value) -> Option<Message> {
    let from = data["week"]["from"].as_str()?;
    let to = data["week"]["to"].as_str()?;
    let mut body = String::new();
    for c in data["currencies"].as_array()? {
        let currency = c["currency"].as_str()?;
        let (income, expenses) = (c["income"].as_str()?, c["expenses"].as_str()?);
        // Writing to a String cannot fail; `let _ =` drops the always-Ok result.
        let _ = writeln!(body, "In: {income} {currency}. Out: {expenses} {currency}.");
    }
    if body.is_empty() {
        body.push_str("No income or spending last week.\n");
    }
    section(&mut body, "Top spending", &data["top_categories"], |c| {
        let name = c["name"].as_str().unwrap_or("No category");
        Some(format!(
            "{name}: {} {}",
            c["expenses"].as_str()?,
            c["currency"].as_str()?
        ))
    })?;
    section(&mut body, "Bills due this week", &data["bills_due"], |b| {
        Some(format!(
            "{} on {}: {} {}",
            b["name"].as_str()?,
            b["due_date"].as_str()?,
            b["amount"].as_str()?,
            b["currency"].as_str()?
        ))
    })?;
    section(&mut body, "Budgets over 80%", &data["budgets"], |b| {
        Some(format!(
            "{}: {}%",
            b["name"].as_str()?,
            b["percent"].as_u64()?
        ))
    })?;
    Some(Message {
        title: format!("Your week in Saldo ({from} to {to})"),
        body: body.trim_end().to_string(),
        link: "/reports".to_string(),
        tag: "digest".to_string(),
    })
}

/// Appends `heading:` and one `- line` per item, or nothing for no items.
fn section(
    body: &mut String,
    heading: &str,
    items: &Value,
    line: impl Fn(&Value) -> Option<String>,
) -> Option<()> {
    let items = items.as_array()?;
    if items.is_empty() {
        return Some(());
    }
    let _ = writeln!(body, "{heading}:");
    for item in items {
        let _ = writeln!(body, "- {}", line(item)?);
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::{AccountNames, Message};

    #[test]
    fn digest_lists_totals_and_only_non_empty_sections() {
        let payload = json!({ "data": {
            "week": { "from": "2026-11-09", "to": "2026-11-15" },
            "currencies": [{ "currency": "EUR", "income": "2000.00", "expenses": "312.40" }],
            "top_categories": [
                { "name": "Groceries", "expenses": "120.00", "currency": "EUR" },
                { "name": null, "expenses": "20.00", "currency": "EUR" }
            ],
            "bills_due": [],
            "budgets": [{ "name": "Fun", "percent": 92 }]
        }});
        let message = Message::for_event("digest.weekly", &payload, &AccountNames::new()).unwrap();
        assert_eq!(
            message.title,
            "Your week in Saldo (2026-11-09 to 2026-11-15)"
        );
        assert_eq!(
            message.body,
            "In: 2000.00 EUR. Out: 312.40 EUR.\nTop spending:\n- Groceries: 120.00 EUR\n\
             - No category: 20.00 EUR\nBudgets over 80%:\n- Fun: 92%"
        );
    }

    #[test]
    fn a_quiet_week_says_so() {
        let payload = json!({ "data": {
            "week": { "from": "2026-11-09", "to": "2026-11-15" },
            "currencies": [], "top_categories": [], "bills_due": [], "budgets": []
        }});
        let message = Message::for_event("digest.weekly", &payload, &AccountNames::new()).unwrap();
        assert_eq!(message.body, "No income or spending last week.");
    }
}
