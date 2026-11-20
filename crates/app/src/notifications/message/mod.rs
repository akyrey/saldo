//! The text of a notification, built from an event's payload. One file per
//! kind of event; [`Message::for_event`] picks the right one.

mod bills;
mod budgets;
mod digest;
mod goals;
mod ledger;

use std::collections::HashMap;

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

/// An event type a channel can subscribe to, as `GET /notification-types` lists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
pub struct NotificationType {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

const fn kind(
    event_type: &'static str,
    label: &'static str,
    description: &'static str,
) -> NotificationType {
    NotificationType {
        event_type,
        label,
        description,
    }
}

/// Every event type that can notify, in the order the settings page shows them.
pub const NOTIFICATION_TYPES: [NotificationType; 9] = [
    kind(
        "bill.due_soon",
        "Bill due soon",
        "A few days before a bill is due.",
    ),
    kind(
        "bill.overdue",
        "Bill overdue",
        "Repeated until an overdue bill is paid.",
    ),
    kind(
        "budget.threshold_reached",
        "Budget nearly used",
        "When a budget reaches 80% and 100% of what it allows.",
    ),
    kind(
        "forecast.below_zero",
        "Balance going below zero",
        "When the forecast shows an account below zero in the next 30 days.",
    ),
    kind(
        "transaction.large",
        "Large transaction",
        "When a transaction at or above the household's large-transaction amount is entered.",
    ),
    kind(
        "recurrence.fired",
        "Recurring transaction created",
        "When a recurring transaction is booked on its date.",
    ),
    kind(
        "piggy_bank.target_reached",
        "Piggy bank goal reached",
        "When a piggy bank reaches its target amount.",
    ),
    kind(
        "net_worth_goal.reached",
        "Net worth goal reached",
        "When net worth reaches one of your goals.",
    ),
    kind(
        "digest.weekly",
        "Weekly digest",
        "Mondays at 08:00: last week's income, spending, top categories and what is coming up.",
    ),
];

/// The event type names in [`NOTIFICATION_TYPES`].
pub const NOTIFYING_EVENTS: [&str; 9] = [
    "bill.due_soon",
    "bill.overdue",
    "budget.threshold_reached",
    "forecast.below_zero",
    "transaction.large",
    "recurrence.fired",
    "piggy_bank.target_reached",
    "net_worth_goal.reached",
    "digest.weekly",
];

/// What a new channel receives when it does not say: bill reminders, as
/// every channel did before Phase 10.
pub const DEFAULT_EVENT_TYPES: [&str; 2] = ["bill.due_soon", "bill.overdue"];

/// Account names by ID, for messages about transactions (payloads carry IDs).
pub type AccountNames = HashMap<String, String>;

/// Whether an event notifies at all. A recurrence notifies only when the
/// scheduler booked a new transaction: a manual "Fire now" is seen by the
/// person who clicked it, and a repeated firing created nothing.
pub fn notifies(event_type: &str, origin: &str, payload: &Value) -> bool {
    if !NOTIFYING_EVENTS.contains(&event_type) {
        return false;
    }
    if event_type == "recurrence.fired" {
        return origin == "scheduler" && payload["data"]["created"] == Value::Bool(true);
    }
    true
}

impl Message {
    /// The message of a `POST /notification-channels/{id}/test`.
    pub fn test() -> Self {
        Self {
            title: "Saldo test notification".to_string(),
            body: "This channel works. The notifications you chose will arrive here.".to_string(),
            link: "/settings/notifications".to_string(),
            tag: "saldo-test".to_string(),
        }
    }

    /// The message for an event, or `None` for event types that do not
    /// notify or a payload without the expected fields.
    pub fn for_event(event_type: &str, payload: &Value, accounts: &AccountNames) -> Option<Self> {
        let data = &payload["data"];
        match event_type {
            "bill.due_soon" | "bill.overdue" => bills::message(event_type, data),
            "budget.threshold_reached" => budgets::threshold(data),
            "forecast.below_zero" => budgets::below_zero(data),
            "transaction.large" => ledger::large(data, accounts),
            "recurrence.fired" => ledger::recurrence(data, accounts),
            "piggy_bank.target_reached" => goals::piggy_bank(data),
            "net_worth_goal.reached" => goals::net_worth(data),
            "digest.weekly" => digest::weekly(data),
            _ => None,
        }
    }
}

/// `word` or `words`.
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

    #[test]
    fn the_test_message_points_at_the_settings_page() {
        assert_eq!(Message::test().link, "/settings/notifications");
    }

    #[test]
    fn the_type_list_and_the_names_agree() {
        let names: Vec<_> = NOTIFICATION_TYPES.iter().map(|t| t.event_type).collect();
        assert_eq!(names, NOTIFYING_EVENTS);
        assert!(
            DEFAULT_EVENT_TYPES
                .iter()
                .all(|t| NOTIFYING_EVENTS.contains(t))
        );
    }

    #[test]
    fn recurrences_notify_only_for_new_scheduled_bookings() {
        let created = json!({ "data": { "created": true } });
        let again = json!({ "data": { "created": false } });
        assert!(notifies("recurrence.fired", "scheduler", &created));
        assert!(!notifies("recurrence.fired", "api", &created));
        assert!(!notifies("recurrence.fired", "scheduler", &again));
        assert!(notifies("bill.overdue", "scheduler", &json!({})));
        assert!(!notifies("bill.paid", "api", &json!({})));
    }

    #[test]
    fn unknown_events_give_no_message() {
        assert!(Message::for_event("bill.paid", &json!({}), &AccountNames::new()).is_none());
    }
}
