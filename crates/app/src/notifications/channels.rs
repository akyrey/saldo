//! Notification channels: each belongs to one member of a household, who is
//! the only one who sees and changes it.

use serde::Serialize;
use serde_json::Value;
use sqlx::SqliteConnection;
use uuid::Uuid;

use super::NotificationService;
use super::config::{ChannelConfig, ChannelKind, ChannelSettings};
use super::message::{DEFAULT_EVENT_TYPES, NOTIFYING_EVENTS};
use crate::clock::{format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::AppError;
use crate::events::{NewEvent, append_event};
use crate::identity::required;

/// A channel as the API returns it: no tokens or push keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct ChannelView {
    pub id: String,
    pub user_id: String,
    /// `ntfy`, `gotify`, `email` or `webpush`.
    pub kind: String,
    pub name: String,
    pub active: bool,
    pub settings: ChannelSettings,
    /// The event types this channel receives (see `GET /notification-types`).
    pub event_types: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A channel with its secrets, for sending. Never serialised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub id: String,
    pub household_id: String,
    pub user_id: String,
    pub name: String,
    pub active: bool,
    pub config: ChannelConfig,
}

#[derive(Debug, Clone)]
pub struct NewChannel {
    pub kind: String,
    pub name: String,
    pub config: Value,
    pub active: Option<bool>,
    /// `None`: bill reminders only, as before Phase 10.
    pub event_types: Option<Vec<String>>,
}

/// A PATCH; `config` fields replace the stored ones (see [`ChannelConfig::merged`]).
#[derive(Debug, Clone, Default)]
pub struct ChannelUpdate {
    pub name: Option<String>,
    pub config: Option<Value>,
    pub active: Option<bool>,
    pub event_types: Option<Vec<String>>,
}

pub(super) struct Row {
    id: String,
    household_id: String,
    user_id: String,
    kind: String,
    name: String,
    config: String,
    active: bool,
    event_types: String,
    created_at: String,
    updated_at: String,
}

impl Row {
    fn channel(&self) -> Result<Channel, AppError> {
        Ok(Channel {
            id: self.id.clone(),
            household_id: self.household_id.clone(),
            user_id: self.user_id.clone(),
            name: self.name.clone(),
            active: self.active,
            config: ChannelConfig::from_stored(&self.kind, &self.config)?,
        })
    }

    pub(super) fn view(&self) -> Result<ChannelView, AppError> {
        Ok(ChannelView {
            id: self.id.clone(),
            user_id: self.user_id.clone(),
            kind: self.kind.clone(),
            name: self.name.clone(),
            active: self.active,
            settings: self.channel()?.config.settings(),
            event_types: serde_json::from_str(&self.event_types).map_err(|_| {
                AppError::Internal(format!("corrupt event types of channel {}", self.id))
            })?,
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        })
    }
}

impl NotificationService {
    /// The caller's own channels, oldest first.
    pub async fn list_channels(&self, ctx: &HouseholdCtx) -> Result<Vec<ChannelView>, AppError> {
        let (household, user) = (ctx.household_id.to_string(), ctx.user_id.to_string());
        let rows = sqlx::query_as!(
            Row,
            r#"SELECT id, household_id, user_id, kind, name, config, active AS "active: bool",
                      event_types, created_at, updated_at
               FROM notification_channels WHERE household_id = ? AND user_id = ?
               ORDER BY created_at, id"#,
            household,
            user
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(Row::view).collect()
    }

    /// One of the caller's channels, with its secrets (for a test send).
    pub async fn channel(&self, ctx: &HouseholdCtx, id: Uuid) -> Result<Channel, AppError> {
        let mut conn = self.pool.acquire().await?;
        find(&mut conn, ctx, id).await?.channel()
    }

    pub async fn create_channel(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewChannel,
    ) -> Result<ChannelView, AppError> {
        let kind = ChannelKind::parse(&input.kind)?;
        let name = required("name", &input.name)?;
        let config = ChannelConfig::parse(kind, &input.config)?
            .to_json()
            .to_string();
        let id = new_id(&*self.clock);
        let (id_text, household, user) = (
            id.to_string(),
            ctx.household_id.to_string(),
            ctx.user_id.to_string(),
        );
        let (now, kind, active) = (
            format_timestamp(self.clock.now()),
            kind.as_str(),
            input.active.unwrap_or(true),
        );
        let defaults = DEFAULT_EVENT_TYPES.map(str::to_string).to_vec();
        let event_types = event_types_json(&input.event_types.unwrap_or(defaults))?;
        let mut tx = crate::db::begin(&self.pool).await?;
        sqlx::query!(
            "INSERT INTO notification_channels (id, household_id, user_id, kind, name, config, active, event_types, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            id_text, household, user, kind, name, config, active, event_types, now, now,
        )
        .execute(&mut *tx)
        .await?;
        let view = find(&mut tx, ctx, id).await?.view()?;
        self.emit(&mut tx, ctx, origin, "created", &view).await?;
        tx.commit().await?;
        Ok(view)
    }

    pub async fn update_channel(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: Uuid,
        update: ChannelUpdate,
    ) -> Result<ChannelView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let row = find(&mut tx, ctx, id).await?;
        let current = row.channel()?;
        let name = match update.name {
            Some(name) => required("name", &name)?,
            None => row.name.clone(),
        };
        let config = match update.config {
            Some(patch) => current.config.merged(&patch)?,
            None => current.config,
        };
        let config = config.to_json().to_string();
        let active = update.active.unwrap_or(row.active);
        let event_types = match &update.event_types {
            Some(types) => event_types_json(types)?,
            None => row.event_types.clone(),
        };
        let now = format_timestamp(self.clock.now());
        sqlx::query!(
            "UPDATE notification_channels SET name = ?, config = ?, active = ?, event_types = ?, updated_at = ?
             WHERE id = ? AND household_id = ? AND user_id = ?",
            name,
            config,
            active,
            event_types,
            now,
            row.id,
            row.household_id,
            row.user_id,
        )
        .execute(&mut *tx)
        .await?;
        let view = find(&mut tx, ctx, id).await?.view()?;
        self.emit(&mut tx, ctx, origin, "updated", &view).await?;
        tx.commit().await?;
        Ok(view)
    }

    /// Deletes the channel and its notification history.
    pub async fn delete_channel(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: Uuid,
    ) -> Result<(), AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let row = find(&mut tx, ctx, id).await?;
        let view = row.view()?;
        sqlx::query!(
            "DELETE FROM notification_channels WHERE id = ? AND household_id = ? AND user_id = ?",
            row.id,
            row.household_id,
            row.user_id
        )
        .execute(&mut *tx)
        .await?;
        self.emit(&mut tx, ctx, origin, "deleted", &view).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn emit(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        verb: &str,
        view: &ChannelView,
    ) -> Result<(), AppError> {
        let event = NewEvent::new(
            &*self.clock,
            "notification_channel",
            verb,
            view.id.clone(),
            view,
        )?;
        append_event(conn, ctx, origin, event).await?;
        Ok(())
    }
}

/// The caller's channel `id`; another member's or household's is `NotFound`.
/// Checks the chosen event types and returns them as stored JSON, without
/// repeats, in the order given.
fn event_types_json(types: &[String]) -> Result<String, AppError> {
    let mut chosen: Vec<&str> = Vec::new();
    for event_type in types {
        let event_type = event_type.trim();
        if !NOTIFYING_EVENTS.contains(&event_type) {
            return Err(AppError::invalid(
                "event_types",
                format!("{event_type:?} is not a notifying event type"),
            ));
        }
        if !chosen.contains(&event_type) {
            chosen.push(event_type);
        }
    }
    serde_json::to_string(&chosen).map_err(|e| AppError::Internal(e.to_string()))
}

pub(super) async fn find(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    id: Uuid,
) -> Result<Row, AppError> {
    let (id, household, user) = (
        id.to_string(),
        ctx.household_id.to_string(),
        ctx.user_id.to_string(),
    );
    sqlx::query_as!(
        Row,
        r#"SELECT id, household_id, user_id, kind, name, config, active AS "active: bool",
                  event_types, created_at, updated_at
           FROM notification_channels WHERE id = ? AND household_id = ? AND user_id = ?"#,
        id,
        household,
        user
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}
