//! A Web Push subscription the push service no longer knows (HTTP 404 or
//! 410): the browser unsubscribed, or its site data was cleared. Retrying
//! cannot help, so the notification fails at once and the channel is paused.

use sqlx::SqliteConnection;
use uuid::Uuid;

use super::NotificationService;
use super::channels::{Channel, find};
use super::outbox::MAX_ERROR;
use crate::clock::format_timestamp;
use crate::ctx::{HouseholdCtx, Origin, Role};
use crate::error::{AppError, parse_stored};

impl NotificationService {
    /// Fails `notification_id` without further tries and pauses `channel`
    /// (`active = false`), appending `notification_channel.updated` with
    /// origin `system` in the same transaction. The member can turn the
    /// channel back on, or remove it and subscribe this browser again.
    pub async fn pause_gone_channel(
        &self,
        notification_id: &str,
        channel: &Channel,
        error: &str,
    ) -> Result<(), AppError> {
        let error: String = error.chars().take(MAX_ERROR).collect();
        let mut tx = crate::db::begin(&self.pool).await?;
        sqlx::query!(
            "UPDATE notifications SET status = 'failed', attempts = attempts + 1, last_error = ?
             WHERE id = ? AND status = 'pending'",
            error,
            notification_id,
        )
        .execute(&mut *tx)
        .await?;
        if let Some(ctx) = owner_of(&mut tx, channel).await? {
            self.pause(&mut tx, &ctx, channel).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn pause(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        channel: &Channel,
    ) -> Result<(), AppError> {
        let now = format_timestamp(self.clock.now());
        let paused = sqlx::query!(
            "UPDATE notification_channels SET active = 0, updated_at = ?
             WHERE id = ? AND household_id = ? AND user_id = ? AND active = 1",
            now,
            channel.id,
            channel.household_id,
            channel.user_id,
        )
        .execute(&mut *conn)
        .await?
        .rows_affected();
        if paused == 0 {
            return Ok(()); // already paused (or deleted): nothing changed
        }
        let id: Uuid = parse_stored(&channel.id)?;
        let view = find(conn, ctx, id).await?.view()?;
        self.emit(conn, ctx, Origin::System, "updated", &view).await
    }
}

/// The context of the member who owns the channel, as the actor of the
/// event; `None` if they are no longer a member.
async fn owner_of(
    conn: &mut SqliteConnection,
    channel: &Channel,
) -> Result<Option<HouseholdCtx>, AppError> {
    let role = sqlx::query_scalar!(
        "SELECT role FROM household_members WHERE household_id = ? AND user_id = ?",
        channel.household_id,
        channel.user_id,
    )
    .fetch_optional(&mut *conn)
    .await?;
    let Some(role) = role else {
        return Ok(None);
    };
    Ok(Some(HouseholdCtx {
        household_id: parse_stored(&channel.household_id)?,
        user_id: parse_stored(&channel.user_id)?,
        role: Role::parse(&role)?,
    }))
}
