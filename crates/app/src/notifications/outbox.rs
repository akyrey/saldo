//! The notifier's bookkeeping (ARCHITECTURE.md 8.4): turning events into
//! `notifications` rows, and recording how each send went.
//!
//! This is a background consumer of the whole event log, so it works across
//! households: every row it writes carries the household of its event, and
//! channels are looked up by that household only.

use serde_json::Value;
use sqlx::SqliteConnection;

use super::NotificationService;
use super::channels::Channel;
use super::config::ChannelConfig;
use super::message::{AccountNames, Message, notifies};
use crate::clock::{format_timestamp, new_id};
use crate::error::AppError;

/// The row in `consumer_cursors`.
pub const CURSOR: &str = "notifications";
/// The first send plus three retries; then the notification is `failed`.
pub const MAX_ATTEMPTS: i64 = 4;
/// Longest stored error message.
const MAX_ERROR: usize = 500;

/// A notification waiting to be sent.
#[derive(Debug, Clone)]
pub struct PendingNotification {
    pub id: String,
    pub attempts: i64,
    pub channel: Channel,
    /// `None` when the event cannot be turned into text (then it fails).
    pub message: Option<Message>,
}

impl NotificationService {
    /// Reads up to `batch` events after the cursor, inserts one notification
    /// per active channel of each household member for those that notify,
    /// and moves the cursor, all in one transaction: a crash loses nothing,
    /// and replays are absorbed by the `(channel_id, dedupe_key)` key.
    /// Returns how many events were read (fewer than `batch`: caught up).
    pub async fn fan_out(&self, batch: i64) -> Result<usize, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let cursor = sqlx::query_scalar!(
            "SELECT last_seq FROM consumer_cursors WHERE consumer = ?",
            CURSOR
        )
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or(0);
        let events = sqlx::query!(
            "SELECT seq, id, household_id, event_type, origin, payload FROM events WHERE seq > ? ORDER BY seq LIMIT ?",
            cursor,
            batch
        )
        .fetch_all(&mut *tx)
        .await?;
        let Some(last) = events.last().map(|e| e.seq) else {
            return Ok(0);
        };
        for event in &events {
            let payload: Value = serde_json::from_str(&event.payload).unwrap_or(Value::Null);
            if !notifies(&event.event_type, &event.origin, &payload) {
                continue;
            }
            let key = dedupe_key(&event.event_type, &payload, &event.id);
            self.enqueue(
                &mut tx,
                &event.household_id,
                &event.event_type,
                event.seq,
                &key,
            )
            .await?;
        }
        let now = format_timestamp(self.clock.now());
        sqlx::query!(
            "INSERT INTO consumer_cursors (consumer, last_seq, updated_at) VALUES (?, ?, ?)
             ON CONFLICT (consumer) DO UPDATE SET last_seq = excluded.last_seq, updated_at = excluded.updated_at",
            CURSOR,
            last,
            now
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(events.len())
    }

    async fn enqueue(
        &self,
        conn: &mut SqliteConnection,
        household: &str,
        event_type: &str,
        seq: i64,
        key: &str,
    ) -> Result<(), AppError> {
        // Active channels of current members that chose this event type: a
        // removed member's channels stay silent.
        let channels = sqlx::query_scalar!(
            "SELECT c.id FROM notification_channels c
             JOIN household_members m ON m.household_id = c.household_id AND m.user_id = c.user_id
             WHERE c.household_id = ? AND c.active = 1
               AND EXISTS (SELECT 1 FROM json_each(c.event_types) t WHERE t.value = ?)
             ORDER BY c.id",
            household,
            event_type
        )
        .fetch_all(&mut *conn)
        .await?;
        let now = format_timestamp(self.clock.now());
        for channel in channels {
            let id = new_id(&*self.clock).to_string();
            sqlx::query!(
                "INSERT INTO notifications (id, household_id, channel_id, event_seq, dedupe_key, created_at)
                 VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT (channel_id, dedupe_key) DO NOTHING",
                id, household, channel, seq, key, now,
            )
            .execute(&mut *conn)
            .await?;
        }
        Ok(())
    }

    /// Up to `limit` pending notifications of active channels, oldest first.
    pub async fn pending(&self, limit: i64) -> Result<Vec<PendingNotification>, AppError> {
        let rows = sqlx::query!(
            r#"SELECT n.id, n.attempts, c.id AS channel_id, c.household_id, c.user_id, c.kind,
                      c.name, c.config, e.event_type, e.payload
               FROM notifications n
               JOIN notification_channels c ON c.id = n.channel_id AND c.household_id = n.household_id
               JOIN events e ON e.seq = n.event_seq
               WHERE n.status = 'pending' AND c.active = 1
               ORDER BY n.created_at, n.id LIMIT ?"#,
            limit
        )
        .fetch_all(&self.pool)
        .await?;
        let mut pending = Vec::with_capacity(rows.len());
        for row in rows {
            // Configs are validated on write; a corrupt one fails its
            // notification instead of blocking every other.
            let config = match ChannelConfig::from_stored(&row.kind, &row.config) {
                Ok(config) => config,
                Err(e) => {
                    self.mark_attempt_failed(&row.id, &e.to_string()).await?;
                    continue;
                }
            };
            let payload: Value = serde_json::from_str(&row.payload).unwrap_or(Value::Null);
            let accounts = account_names(&self.pool, &row.household_id, &payload).await?;
            pending.push(PendingNotification {
                id: row.id,
                attempts: row.attempts,
                channel: Channel {
                    id: row.channel_id,
                    household_id: row.household_id,
                    user_id: row.user_id,
                    name: row.name,
                    active: true,
                    config,
                },
                message: Message::for_event(&row.event_type, &payload, &accounts),
            });
        }
        Ok(pending)
    }

    pub async fn mark_sent(&self, id: &str) -> Result<(), AppError> {
        let now = format_timestamp(self.clock.now());
        sqlx::query!(
            "UPDATE notifications SET status = 'sent', attempts = attempts + 1, sent_at = ?, last_error = NULL
             WHERE id = ? AND status = 'pending'",
            now,
            id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Records a failed attempt; after [`MAX_ATTEMPTS`] the notification is
    /// `failed`. Returns whether it gave up.
    pub async fn mark_attempt_failed(&self, id: &str, error: &str) -> Result<bool, AppError> {
        let error: String = error.chars().take(MAX_ERROR).collect();
        let status = sqlx::query_scalar!(
            "UPDATE notifications SET attempts = attempts + 1, last_error = ?,
                    status = CASE WHEN attempts + 1 >= ? THEN 'failed' ELSE 'pending' END
             WHERE id = ? AND status = 'pending' RETURNING status",
            error,
            MAX_ATTEMPTS,
            id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(status.as_deref() == Some("failed"))
    }
}

/// The names of the accounts a transaction payload refers to (its splits'
/// sources and destinations), for the message text. Empty for other events.
async fn account_names(
    pool: &sqlx::SqlitePool,
    household: &str,
    payload: &Value,
) -> Result<AccountNames, AppError> {
    let Some(splits) = payload["data"]["group"]["splits"].as_array() else {
        return Ok(AccountNames::new());
    };
    let ids: Vec<&str> = splits
        .iter()
        .flat_map(|s| [&s["source_account_id"], &s["destination_account_id"]])
        .filter_map(Value::as_str)
        .collect();
    let ids = serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string());
    let rows = sqlx::query!(
        "SELECT id, name FROM accounts WHERE household_id = ? AND id IN (SELECT value FROM json_each(?))",
        household,
        ids
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.name)).collect())
}

/// One notification per channel and occurrence, however often the event is
/// read: `bill.overdue:<period id>:<reminder day>`, `<type>:<alert key>` for
/// scheduler alerts, `transaction.large:<split id>`,
/// `recurrence.fired:<recurrence id>:<date>`, else `<type>:<event id>`.
fn dedupe_key(event_type: &str, payload: &Value, event_id: &str) -> String {
    let data = &payload["data"];
    let period = &data["period"];
    if let (Some(id), Some(day)) = (period["id"].as_str(), period["last_reminded_on"].as_str()) {
        return format!("{event_type}:{id}:{day}");
    }
    if let Some(key) = data["alert_key"].as_str() {
        return format!("{event_type}:{key}");
    }
    if let Some(split) = data["split_id"].as_str() {
        return format!("{event_type}:{split}");
    }
    if let (Some(id), Some(date)) = (data["recurrence_id"].as_str(), data["date"].as_str()) {
        return format!("{event_type}:{id}:{date}");
    }
    format!("{event_type}:{event_id}")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn dedupe_keys_name_the_period_and_the_day() {
        let payload =
            json!({ "data": { "period": { "id": "p1", "last_reminded_on": "2026-11-14" } } });
        assert_eq!(
            dedupe_key("bill.overdue", &payload, "e1"),
            "bill.overdue:p1:2026-11-14"
        );
        assert_eq!(
            dedupe_key("bill.overdue", &json!({}), "e1"),
            "bill.overdue:e1"
        );
    }

    #[test]
    fn dedupe_keys_for_alerts_large_splits_and_recurrences() {
        let alert = json!({ "data": { "alert_key": "budget:l1:80" } });
        assert_eq!(
            dedupe_key("budget.threshold_reached", &alert, "e1"),
            "budget.threshold_reached:budget:l1:80"
        );
        let large = json!({ "data": { "split_id": "s1" } });
        assert_eq!(
            dedupe_key("transaction.large", &large, "e1"),
            "transaction.large:s1"
        );
        let fired = json!({ "data": { "recurrence_id": "r1", "date": "2026-12-01" } });
        assert_eq!(
            dedupe_key("recurrence.fired", &fired, "e1"),
            "recurrence.fired:r1:2026-12-01"
        );
    }
}
