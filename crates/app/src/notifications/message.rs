//! The text of a notification, built from an event's payload.

use chrono::NaiveDate;
use serde_json::Value;

/// What a channel sends. `link` is a path inside the web app (for example
/// `/bills/<id>`); senders prefix it with `SALDO_PUBLIC_URL` where they need
/// an absolute URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub title: String,
    pub body: String,
    pub link: String,
    /// Groups notifications about the same thing (Web Push `tag`).
    pub tag: String,
}

/// The event types that notify (more may follow, ARCHITECTURE.md 8.4).
pub const NOTIFYING_EVENTS: [&str; 2] = ["bill.due_soon", "bill.overdue"];

impl Message {
    /// The message of a `POST /notification-channels/{id}/test`.
    pub fn test() -> Self {
        Self {
            title: "Saldo test notification".to_string(),
            body: "This channel works. Bill reminders will arrive here.".to_string(),
            link: "/settings/notifications".to_string(),
            tag: "saldo-test".to_string(),
        }
    }

    /// The message for an event, or `None` for event types that do not
    /// notify or a payload without the expected fields.
    pub fn for_event(event_type: &str, payload: &Value) -> Option<Self> {
        let data = &payload["data"];
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
        Some(Self {
            title,
            body,
            link: format!("/bills/{}", bill.id),
            tag: format!("bill-{}", bill.id),
        })
    }
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

fn plural(count: i64, word: &str) -> String {
    if count == 1 {
        word.to_string()
    } else {
        format!("{word}s")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

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
        )
        .unwrap();
        assert_eq!(tomorrow.body, "800.00 EUR is due tomorrow (2026-11-20).");
        let today = Message::for_event(
            "bill.due_soon",
            &payload("800.00", "800.00", "2026-11-20", "2026-11-20"),
        )
        .unwrap();
        assert_eq!(today.body, "800.00 EUR is due today (2026-11-20).");
    }

    #[test]
    fn overdue_counts_the_days() {
        let one = Message::for_event(
            "bill.overdue",
            &payload("800.00", "800.00", "2026-11-20", "2026-11-21"),
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
        )
        .unwrap();
        assert!(five.body.ends_with("(5 days overdue)."));
    }

    #[test]
    fn other_events_and_odd_payloads_give_no_message() {
        let ok = payload("1", "1", "2026-11-20", "2026-11-21");
        assert!(Message::for_event("bill.paid", &ok).is_none());
        assert!(Message::for_event("bill.overdue", &json!({})).is_none());
        let bad_date = payload("1", "1", "soon", "2026-11-21");
        assert!(Message::for_event("bill.overdue", &bad_date).is_none());
    }

    #[test]
    fn the_test_message_points_at_the_settings_page() {
        assert_eq!(Message::test().link, "/settings/notifications");
    }
}
