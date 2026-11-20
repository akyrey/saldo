//! Transactions: a large one entered, a recurring one booked.

use serde_json::Value;

use super::{AccountNames, Message, plural};

fn name<'a>(accounts: &'a AccountNames, id: &str) -> &'a str {
    accounts.get(id).map_or("an account", String::as_str)
}

/// `from Checking to Groceries`.
fn route(split: &Value, accounts: &AccountNames) -> Option<String> {
    Some(format!(
        "from {} to {}",
        name(accounts, split["source_account_id"].as_str()?),
        name(accounts, split["destination_account_id"].as_str()?)
    ))
}

/// The group's title, or its first split's description.
fn title_of(group: &Value) -> Option<&str> {
    group["title"]
        .as_str()
        .filter(|t| !t.trim().is_empty())
        .or_else(|| group["splits"][0]["description"].as_str())
}

/// `data`: `{ group, split_id, threshold }`.
pub(super) fn large(data: &Value, accounts: &AccountNames) -> Option<Message> {
    let group = &data["group"];
    let group_id = group["id"].as_str()?;
    let split_id = data["split_id"].as_str()?;
    let split = group["splits"]
        .as_array()?
        .iter()
        .find(|s| s["id"].as_str() == Some(split_id))?;
    let (amount, currency, date) = (
        split["amount"].as_str()?,
        split["currency"].as_str()?,
        split["date"].as_str()?,
    );
    let description = split["description"].as_str()?;
    Some(Message {
        title: format!("Large transaction: {description}"),
        body: format!(
            "{amount} {currency} {} on {date} (your alert is set at {} {currency}).",
            route(split, accounts)?,
            data["threshold"].as_str()?
        ),
        link: format!("/transactions/{group_id}/edit"),
        tag: format!("large-{split_id}"),
    })
}

/// `data`: `{ recurrence_id, date, created, group }`.
pub(super) fn recurrence(data: &Value, accounts: &AccountNames) -> Option<Message> {
    let group = &data["group"];
    let group_id = group["id"].as_str()?;
    let date = data["date"].as_str()?;
    let splits = group["splits"].as_array()?;
    let first = splits.first()?;
    let currency = first["currency"].as_str()?;
    let body = if splits.len() == 1 {
        format!(
            "{} {currency} {} on {date}.",
            first["amount"].as_str()?,
            route(first, accounts)?
        )
    } else {
        let count = i64::try_from(splits.len()).ok()?;
        format!("{count} {} on {date}.", plural(count, "split"))
    };
    Some(Message {
        title: format!("Recurring transaction created: {}", title_of(group)?),
        body,
        link: format!("/transactions/{group_id}/edit"),
        tag: format!("recurrence-{}-{date}", data["recurrence_id"].as_str()?),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::super::{AccountNames, Message};

    fn names() -> AccountNames {
        [("a1", "Checking"), ("a2", "Rent")]
            .into_iter()
            .map(|(id, name)| (id.to_string(), name.to_string()))
            .collect()
    }

    fn split(id: &str, amount: &str) -> Value {
        json!({ "id": id, "amount": amount, "currency": "EUR", "date": "2026-12-01",
                "description": "Flat", "source_account_id": "a1", "destination_account_id": "a2" })
    }

    #[test]
    fn large_names_amount_accounts_and_threshold() {
        let payload = json!({ "data": {
            "group": { "id": "g1", "title": null, "splits": [split("s1", "900.00")] },
            "split_id": "s1",
            "threshold": "500.00"
        }});
        let message = Message::for_event("transaction.large", &payload, &names()).unwrap();
        assert_eq!(message.title, "Large transaction: Flat");
        assert_eq!(
            message.body,
            "900.00 EUR from Checking to Rent on 2026-12-01 (your alert is set at 500.00 EUR)."
        );
        assert_eq!(message.link, "/transactions/g1/edit");
    }

    #[test]
    fn recurrence_with_one_split_shows_its_route() {
        let payload = json!({ "data": {
            "recurrence_id": "r1", "date": "2026-12-01", "created": true,
            "group": { "id": "g1", "title": "Monthly rent", "splits": [split("s1", "800.00")] }
        }});
        let message = Message::for_event("recurrence.fired", &payload, &names()).unwrap();
        assert_eq!(message.title, "Recurring transaction created: Monthly rent");
        assert_eq!(
            message.body,
            "800.00 EUR from Checking to Rent on 2026-12-01."
        );
        assert_eq!(message.tag, "recurrence-r1-2026-12-01");
    }

    #[test]
    fn recurrence_with_several_splits_counts_them_and_unknown_accounts_are_generic() {
        let payload = json!({ "data": {
            "recurrence_id": "r1", "date": "2026-12-01", "created": true,
            "group": { "id": "g1", "title": "", "splits": [split("s1", "1"), split("s2", "2")] }
        }});
        let message = Message::for_event("recurrence.fired", &payload, &names()).unwrap();
        assert_eq!(message.title, "Recurring transaction created: Flat");
        assert_eq!(message.body, "2 splits on 2026-12-01.");
        let single = json!({ "data": {
            "recurrence_id": "r1", "date": "2026-12-01", "created": true,
            "group": { "id": "g1", "title": null, "splits": [split("s1", "5.00")] }
        }});
        let unnamed =
            Message::for_event("recurrence.fired", &single, &AccountNames::new()).unwrap();
        assert_eq!(
            unnamed.body,
            "5.00 EUR from an account to an account on 2026-12-01."
        );
    }
}
