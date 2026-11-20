//! Savings goals: a piggy bank or a net worth goal reached.

use serde_json::Value;

use super::Message;

/// `data`: the piggy bank as the API returns it.
pub(super) fn piggy_bank(data: &Value) -> Option<Message> {
    let id = data["id"].as_str()?;
    let name = data["name"].as_str()?;
    let currency = data["currency"].as_str()?;
    let saved = data["saved_amount"].as_str()?;
    let body = match data["target_amount"].as_str() {
        Some(target) => format!("{saved} {currency} saved of a {target} {currency} target."),
        None => format!("{saved} {currency} saved."),
    };
    Some(Message {
        title: format!("Piggy bank goal reached: {name}"),
        body,
        link: "/piggy-banks".to_string(),
        tag: format!("piggy-{id}"),
    })
}

/// `data`: the goal as the API returns it.
pub(super) fn net_worth(data: &Value) -> Option<Message> {
    let id = data["id"].as_str()?;
    let name = data["name"].as_str()?;
    let target = data["target_amount"].as_str()?;
    let currency = data["currency"].as_str()?;
    Some(Message {
        title: format!("Net worth goal reached: {name}"),
        body: format!("Your net worth reached the {target} {currency} target."),
        link: "/reports/net-worth".to_string(),
        tag: format!("goal-{id}"),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::{AccountNames, Message};

    #[test]
    fn piggy_bank_reached_shows_saved_and_target() {
        let payload = json!({ "data": {
            "id": "p1", "name": "Holiday", "currency": "EUR",
            "saved_amount": "1500.00", "target_amount": "1500.00"
        }});
        let message =
            Message::for_event("piggy_bank.target_reached", &payload, &AccountNames::new())
                .unwrap();
        assert_eq!(message.title, "Piggy bank goal reached: Holiday");
        assert_eq!(message.body, "1500.00 EUR saved of a 1500.00 EUR target.");
    }

    #[test]
    fn net_worth_goal_reached_names_the_target() {
        let payload = json!({ "data": {
            "id": "g1", "name": "Emergency fund", "target_amount": "10000.00", "currency": "EUR"
        }});
        let message =
            Message::for_event("net_worth_goal.reached", &payload, &AccountNames::new()).unwrap();
        assert_eq!(message.title, "Net worth goal reached: Emergency fund");
        assert_eq!(message.link, "/reports/net-worth");
    }
}
