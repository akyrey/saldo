//! Managing endpoints (owner only): CRUD, rotate-secret, test, deliveries,
//! redeliver.

use serde_json::json;
use sqlx::SqliteConnection;

use super::matching::{TEST_EVENT, check_event_types, check_url};
use super::repo::{self, DeliveryView};
use super::{
    NewWebhook, WebhookService, WebhookUpdate, WebhookView, WebhookWithSecret, new_secret,
};
use crate::clock::{format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::db::json_list;
use crate::error::AppError;
use crate::events::{NewEvent, append_event};
use crate::identity::required;
use crate::page::{Page, PageRequest};

impl WebhookService {
    pub async fn list(&self, ctx: &HouseholdCtx) -> Result<Vec<WebhookView>, AppError> {
        ctx.require_owner()?;
        let mut conn = self.pool.acquire().await?;
        repo::list(&mut conn, ctx).await
    }

    pub async fn get(&self, ctx: &HouseholdCtx, id: &str) -> Result<WebhookView, AppError> {
        ctx.require_owner()?;
        let mut conn = self.pool.acquire().await?;
        repo::find(&mut conn, ctx, id).await
    }

    /// Creates an endpoint with a server-generated secret, returned once.
    pub async fn create(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewWebhook,
    ) -> Result<WebhookWithSecret, AppError> {
        ctx.require_owner()?;
        let name = required("name", &input.name)?;
        let url = check_url(&input.url)?;
        let types = check_event_types(&input.event_types.unwrap_or_else(|| vec!["*".into()]))?;
        let types = json_list(&types)?;
        let (id, household_id, secret) = (
            new_id(&*self.clock).to_string(),
            ctx.household_id.to_string(),
            new_secret(),
        );
        let now = format_timestamp(self.clock.now());
        let mut tx = crate::db::begin(&self.pool).await?;
        sqlx::query!(
            "INSERT INTO webhook_endpoints (id, household_id, name, url, secret, event_types,
                 include_import_events, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            id,
            household_id,
            name,
            url,
            secret,
            types,
            input.include_import_events,
            now,
            now,
        )
        .execute(&mut *tx)
        .await?;
        let webhook = self.record(&mut tx, ctx, origin, &id, "created").await?;
        tx.commit().await?;
        Ok(WebhookWithSecret { webhook, secret })
    }

    pub async fn update(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
        update: WebhookUpdate,
    ) -> Result<WebhookView, AppError> {
        ctx.require_owner()?;
        let mut tx = crate::db::begin(&self.pool).await?;
        let current = repo::find(&mut tx, ctx, id).await?;
        let name = match update.name {
            Some(name) => required("name", &name)?,
            None => current.name,
        };
        let url = match update.url {
            Some(url) => check_url(&url)?,
            None => current.url,
        };
        let types = json_list(&match update.event_types {
            Some(types) => check_event_types(&types)?,
            None => current.event_types,
        })?;
        let imports = update
            .include_import_events
            .unwrap_or(current.include_import_events);
        let reactivate = update.active == Some(true) && !current.active;
        let active = update.active.unwrap_or(current.active);
        let household_id = ctx.household_id.to_string();
        let now = format_timestamp(self.clock.now());
        sqlx::query!(
            "UPDATE webhook_endpoints
             SET name = ?, url = ?, event_types = ?, include_import_events = ?, active = ?,
                 consecutive_failures = CASE WHEN ? THEN 0 ELSE consecutive_failures END,
                 disabled_reason = CASE WHEN ? OR ? THEN NULL ELSE disabled_reason END,
                 updated_at = ?
             WHERE household_id = ? AND id = ?",
            name,
            url,
            types,
            imports,
            active,
            reactivate,
            reactivate,
            active,
            now,
            household_id,
            id,
        )
        .execute(&mut *tx)
        .await?;
        let view = self.record(&mut tx, ctx, origin, id, "updated").await?;
        tx.commit().await?;
        Ok(view)
    }

    /// Deletes the endpoint and its delivery log.
    pub async fn delete(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
    ) -> Result<(), AppError> {
        ctx.require_owner()?;
        let mut tx = crate::db::begin(&self.pool).await?;
        let view = repo::find(&mut tx, ctx, id).await?;
        let household_id = ctx.household_id.to_string();
        sqlx::query!(
            "DELETE FROM webhook_endpoints WHERE household_id = ? AND id = ?",
            household_id,
            id
        )
        .execute(&mut *tx)
        .await?;
        let event = NewEvent::new(&*self.clock, "webhook", "deleted", id.to_string(), &view)?;
        append_event(&mut tx, ctx, origin, event).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Replaces the secret; the new one is returned once.
    pub async fn rotate_secret(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
    ) -> Result<WebhookWithSecret, AppError> {
        ctx.require_owner()?;
        let mut tx = crate::db::begin(&self.pool).await?;
        repo::find(&mut tx, ctx, id).await?;
        let (secret, household_id, now) = (
            new_secret(),
            ctx.household_id.to_string(),
            format_timestamp(self.clock.now()),
        );
        sqlx::query!(
            "UPDATE webhook_endpoints SET secret = ?, updated_at = ? WHERE household_id = ? AND id = ?",
            secret,
            now,
            household_id,
            id,
        )
        .execute(&mut *tx)
        .await?;
        let webhook = self.record(&mut tx, ctx, origin, id, "updated").await?;
        tx.commit().await?;
        Ok(WebhookWithSecret { webhook, secret })
    }

    /// Appends a `webhook.test` event that only this endpoint receives.
    /// Returns the event's ID.
    pub async fn test(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
    ) -> Result<String, AppError> {
        ctx.require_owner()?;
        let mut tx = crate::db::begin(&self.pool).await?;
        let webhook = repo::find(&mut tx, ctx, id).await?;
        let data = json!({ "webhook": webhook, "message": "Test delivery from Saldo" });
        let (entity, verb) = TEST_EVENT.split_once('.').unwrap_or(("webhook", "test"));
        let event = NewEvent::new(&*self.clock, entity, verb, id.to_string(), &data)?;
        let event_id = event.id.to_string();
        append_event(&mut tx, ctx, origin, event).await?;
        tx.commit().await?;
        Ok(event_id)
    }

    /// The endpoint's deliveries, newest event first.
    pub async fn deliveries(
        &self,
        ctx: &HouseholdCtx,
        id: &str,
        page: PageRequest,
    ) -> Result<Page<DeliveryView>, AppError> {
        ctx.require_owner()?;
        let mut conn = self.pool.acquire().await?;
        repo::find(&mut conn, ctx, id).await?;
        let (items, total) =
            repo::deliveries(&mut conn, ctx, id, page.limit(), page.offset()).await?;
        Ok(Page::new(items, page, total))
    }

    /// Queues a delivery again, now, with a fresh attempt budget.
    pub async fn redeliver(
        &self,
        ctx: &HouseholdCtx,
        delivery_id: &str,
    ) -> Result<DeliveryView, AppError> {
        ctx.require_owner()?;
        let mut conn = self.pool.acquire().await?;
        repo::find_delivery(&mut conn, ctx, delivery_id).await?;
        let now = format_timestamp(self.clock.now());
        sqlx::query!(
            "UPDATE webhook_deliveries
             SET status = 'pending', attempts = 0, next_attempt_at = ?
             WHERE id = ?",
            now,
            delivery_id,
        )
        .execute(&mut *conn)
        .await?;
        repo::find_delivery(&mut conn, ctx, delivery_id).await
    }

    /// Reads the endpoint back and appends `webhook.<verb>` with it.
    async fn record(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
        verb: &str,
    ) -> Result<WebhookView, AppError> {
        let view = repo::find(&mut *conn, ctx, id).await?;
        let event = NewEvent::new(&*self.clock, "webhook", verb, id.to_string(), &view)?;
        append_event(&mut *conn, ctx, origin, event).await?;
        Ok(view)
    }
}
