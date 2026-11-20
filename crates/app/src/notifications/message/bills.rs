//! Bill reminders: due soon and overdue.

use chrono::NaiveDate;
use serde_json::Value;

use super::{Message, plural};

/// `data` is the payload's `data`: `{ bill, period }`.
pub(super) fn message(event_type: &str, data: &Value) -> Option<Message> {
    let bill = Bill::read(&data["bill"])?;
    let due: NaiveDate = data["period"]["due_date"].as_str()?.parse().ok()?;
    let today: NaiveDate = data["period"]["last_reminded_on"].as_str()?.parse().ok()?;
    let (title, body) = match event_type {
        "bill.due_soon" => (
            format!("Bill due soon: {}", bill.name),
            format!("{} {}.", bill.amount(), due_soon_text(due, today)),
        ),
        "bill.overdue" => {
            let days = (today - due).num_days();
            (
                format!("Bill overdue: {}", bill.name),
                format!(
                    "{} was due on {due} ({days} {} overdue).",
                    bill.amount(),
                    plural(days, "day")
                ),
            )
        }
        _ => return None,
    };
    Some(Message {
        title,
        body,
        link: format!("/bills/{}", bill.id),
        tag: format!("bill-{}", bill.id),
    })
}

struct Bill<'a> {
    id: &'a str,
    name: &'a str,
    min: &'a str,
    max: &'a str,
    currency: &'a str,
}

impl<'a> Bill<'a> {
    /// The `'a` says the borrowed strings live as long as the payload they
    /// point into: no copies, and the compiler checks they are not kept
    /// after the payload is gone.
    fn read(value: &'a Value) -> Option<Self> {
        Some(Self {
            id: value["id"].as_str()?,
            name: value["name"].as_str()?,
            min: value["amount_min"].as_str()?,
            max: value["amount_max"].as_str()?,
            currency: value["currency"].as_str()?,
        })
    }

    /// `29.90–35.00 EUR`, or `800.00 EUR` for a fixed amount.
    fn amount(&self) -> String {
        if self.min == self.max {
            format!("{} {}", self.min, self.currency)
        } else {
            format!("{}–{} {}", self.min, self.max, self.currency)
        }
    }
}

fn due_soon_text(due: NaiveDate, today: NaiveDate) -> String {
    match (due - today).num_days() {
        0 => format!("is due today ({due})"),
        1 => format!("is due tomorrow ({due})"),
        days => format!("is due in {days} days ({due})"),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::AccountNames;
    use super::*;

    fn payload(min: &str, max: &str, due: &str, reminded: &str) -> Value {
        json!({ "data": {
            "bill": { "id": "b1", "name": "Rent", "amount_min": min, "amount_max": max, "currency": "EUR" },
            "period": { "due_date": due, "last_reminded_on": reminded }
        }})
    }

    #[test]
    fn due_soon_names_the_bill_the_amount_range_and_the_due_date() {
        let message = Message::for_event(
            "bill.due_soon",
            &payload("29.90", "35.00", "2026-11-20", "2026-11-17"),
            &AccountNames::new(),
        )
        .unwrap();
        assert_eq!(message.title, "Bill due soon: Rent");
        assert_eq!(
            message.body,
            "29.90–35.00 EUR is due in 3 days (2026-11-20)."
        );
        assert_eq!(message.link, "/bills/b1");
        assert_eq!(message.tag, "bill-b1");
        let tomorrow = Message::for_event(
            "bill.due_soon",
            &payload("800.00", "800.00", "2026-11-20", "2026-11-19"),
            &AccountNames::new(),
        )
        .unwrap();
        assert_eq!(tomorrow.body, "800.00 EUR is due tomorrow (2026-11-20).");
        let today = Message::for_event(
            "bill.due_soon",
            &payload("800.00", "800.00", "2026-11-20", "2026-11-20"),
            &AccountNames::new(),
        )
        .unwrap();
        assert_eq!(today.body, "800.00 EUR is due today (2026-11-20).");
    }

    #[test]
    fn overdue_counts_the_days() {
        let one = Message::for_event(
            "bill.overdue",
            &payload("800.00", "800.00", "2026-11-20", "2026-11-21"),
            &AccountNames::new(),
        )
        .unwrap();
        assert_eq!(one.title, "Bill overdue: Rent");
        assert_eq!(
            one.body,
            "800.00 EUR was due on 2026-11-20 (1 day overdue)."
        );
        let five = Message::for_event(
            "bill.overdue",
            &payload("800.00", "800.00", "2026-11-20", "2026-11-25"),
            &AccountNames::new(),
        )
        .unwrap();
        assert!(five.body.ends_with("(5 days overdue)."));
    }

    #[test]
    fn other_events_and_odd_payloads_give_no_message() {
        let ok = payload("1", "1", "2026-11-20", "2026-11-21");
        assert!(Message::for_event("bill.paid", &ok, &AccountNames::new()).is_none());
        assert!(Message::for_event("bill.overdue", &json!({}), &AccountNames::new()).is_none());
        let bad_date = payload("1", "1", "soon", "2026-11-21");
        assert!(Message::for_event("bill.overdue", &bad_date, &AccountNames::new()).is_none());
    }
}
