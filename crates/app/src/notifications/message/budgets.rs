//! Budget thresholds and the forecast alert.

use serde_json::Value;

use super::Message;

/// `data`: `{ budget: { id, name }, limit, threshold, total }`.
pub(super) fn threshold(data: &Value) -> Option<Message> {
    let id = data["budget"]["id"].as_str()?;
    let name = data["budget"]["name"].as_str()?;
    let limit = &data["limit"];
    let (spent, left, currency) = (
        limit["spent"].as_str()?,
        limit["available"].as_str()?,
        limit["currency"].as_str()?,
    );
    let (total, end) = (data["total"].as_str()?, limit["end_date"].as_str()?);
    let percent = data["threshold"].as_u64()?;
    let title = if percent >= 100 {
        format!("Budget used up: {name}")
    } else {
        format!("Budget at {percent}%: {name}")
    };
    Some(Message {
        title,
        body: format!("{spent} of {total} {currency} spent; {left} {currency} left until {end}."),
        link: format!("/budgets/{id}"),
        tag: format!("budget-{id}"),
    })
}

/// `data`: `{ account: { id, name }, currency, lowest: { date, balance } }`.
pub(super) fn below_zero(data: &Value) -> Option<Message> {
    let id = data["account"]["id"].as_str()?;
    let name = data["account"]["name"].as_str()?;
    let currency = data["currency"].as_str()?;
    let (date, balance) = (
        data["lowest"]["date"].as_str()?,
        data["lowest"]["balance"].as_str()?,
    );
    Some(Message {
        title: format!("Balance going below zero: {name}"),
        body: format!("The forecast reaches {balance} {currency} on {date}."),
        link: "/forecast".to_string(),
        tag: format!("forecast-{id}"),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::{AccountNames, Message};

    fn budget(threshold: u64) -> serde_json::Value {
        json!({ "data": {
            "budget": { "id": "b1", "name": "Groceries" },
            "limit": { "spent": "170.00", "available": "30.00", "currency": "EUR", "end_date": "2026-11-30" },
            "threshold": threshold,
            "total": "200.00"
        }})
    }

    #[test]
    fn budget_threshold_says_what_is_spent_and_left() {
        let names = AccountNames::new();
        let warn = Message::for_event("budget.threshold_reached", &budget(80), &names).unwrap();
        assert_eq!(warn.title, "Budget at 80%: Groceries");
        assert_eq!(
            warn.body,
            "170.00 of 200.00 EUR spent; 30.00 EUR left until 2026-11-30."
        );
        assert_eq!(warn.link, "/budgets/b1");
        let full = Message::for_event("budget.threshold_reached", &budget(100), &names).unwrap();
        assert_eq!(full.title, "Budget used up: Groceries");
    }

    #[test]
    fn below_zero_names_the_account_date_and_balance() {
        let payload = json!({ "data": {
            "account": { "id": "a1", "name": "Checking" },
            "currency": "EUR",
            "lowest": { "date": "2026-12-03", "balance": "-42.10" }
        }});
        let message =
            Message::for_event("forecast.below_zero", &payload, &AccountNames::new()).unwrap();
        assert_eq!(message.title, "Balance going below zero: Checking");
        assert_eq!(
            message.body,
            "The forecast reaches -42.10 EUR on 2026-12-03."
        );
        assert_eq!(message.tag, "forecast-a1");
    }
}
