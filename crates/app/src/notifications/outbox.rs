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
use super::message::{Message, NOTIFYING_EVENTS};
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
        let mut tx = self.pool.begin().await?;
        let cursor = sqlx::query_scalar!(
            "SELECT last_seq FROM consumer_cursors WHERE consumer = ?",
            CURSOR
        )
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or(0);
        let events = sqlx::query!(
            "SELECT seq, id, household_id, event_type, payload FROM events WHERE seq > ? ORDER BY seq LIMIT ?",
            cursor,
            batch
        )
        .fetch_all(&mut *tx)
        .await?;
        let Some(last) = events.last().map(|e| e.seq) else {
            return Ok(0);
        };
        for event in &events {
            if !NOTIFYING_EVENTS.contains(&event.event_type.as_str()) {
                continue;
            }
            let payload: Value = serde_json::from_str(&event.payload).unwrap_or(Value::Null);
            let key = dedupe_key(&event.event_type, &payload, &event.id);
            self.enqueue(&mut tx, &event.household_id, event.seq, &key)
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
        seq: i64,
        key: &str,
    ) -> Result<(), AppError> {
        // Active channels of current members only: a removed member's
        // channels stay silent.
        let channels = sqlx::query_scalar!(
            "SELECT c.id FROM notification_channels c
             JOIN household_members m ON m.household_id = c.household_id AND m.user_id = c.user_id
             WHERE c.household_id = ? AND c.active = 1 ORDER BY c.id",
            household
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
                message: Message::for_event(&row.event_type, &payload),
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

/// `bill.overdue:<period id>:<day>`: one notification per channel, period and
/// reminder day, however often the event is read.
fn dedupe_key(event_type: &str, payload: &Value, event_id: &str) -> String {
    let period = &payload["data"]["period"];
    match (period["id"].as_str(), period["last_reminded_on"].as_str()) {
        (Some(id), Some(day)) => format!("{event_type}:{id}:{day}"),
        _ => format!("{event_type}:{event_id}"),
    }
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
}
