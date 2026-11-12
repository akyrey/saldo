//! Tags: a name per household, optionally with a description.

use saldo_core::TagId;
use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};

use crate::clock::{SharedClock, format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::AppError;
use crate::events::{NewEvent, append_event};
use crate::identity::required;
use crate::page::{Page, PageRequest};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct TagView {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct NewTag {
    pub name: String,
    pub description: Option<String>,
}

/// `None` keeps a value; `description: Some(None)` clears the description.
#[derive(Debug, Clone, Default)]
pub struct TagUpdate {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
}

const DUPLICATE: &str = "a tag with this name exists";

#[derive(Clone)]
pub struct TagService {
    pool: SqlitePool,
    clock: SharedClock,
}

impl TagService {
    pub fn new(pool: SqlitePool, clock: SharedClock) -> Self {
        Self { pool, clock }
    }

    pub async fn create(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewTag,
    ) -> Result<TagView, AppError> {
        let name = required("name", &input.name)?;
        let (id, household_id) = (
            new_id(&*self.clock).to_string(),
            ctx.household_id.to_string(),
        );
        let now = format_timestamp(self.clock.now());
        let mut tx = self.pool.begin().await?;
        sqlx::query!(
            "INSERT INTO tags (id, household_id, name, description, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            id,
            household_id,
            name,
            input.description,
            now,
            now,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::conflict_on_unique(e, DUPLICATE))?;
        let view = find(&mut tx, ctx, &id).await?;
        append_event(
            &mut tx,
            ctx,
            origin,
            NewEvent::new(&*self.clock, "tag", "created", id, &view)?,
        )
        .await?;
        tx.commit().await?;
        Ok(view)
    }

    pub async fn update(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: TagId,
        update: TagUpdate,
    ) -> Result<TagView, AppError> {
        let id = id.to_string();
        let mut tx = self.pool.begin().await?;
        let current = find(&mut tx, ctx, &id).await?;
        let name = match update.name {
            Some(name) => required("name", &name)?,
            None => current.name,
        };
        let description = update.description.unwrap_or(current.description);
        let (household_id, now) = (
            ctx.household_id.to_string(),
            format_timestamp(self.clock.now()),
        );
        sqlx::query!(
            "UPDATE tags SET name = ?, description = ?, updated_at = ? WHERE household_id = ? AND id = ?",
            name, description, now, household_id, id,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::conflict_on_unique(e, DUPLICATE))?;
        let view = find(&mut tx, ctx, &id).await?;
        append_event(
            &mut tx,
            ctx,
            origin,
            NewEvent::new(&*self.clock, "tag", "updated", id, &view)?,
        )
        .await?;
        tx.commit().await?;
        Ok(view)
    }

    /// Deletes the tag and removes it from every split.
    pub async fn delete(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: TagId,
    ) -> Result<(), AppError> {
        let id = id.to_string();
        let mut tx = self.pool.begin().await?;
        let view = find(&mut tx, ctx, &id).await?;
        let household_id = ctx.household_id.to_string();
        sqlx::query!(
            "DELETE FROM tags WHERE household_id = ? AND id = ?",
            household_id,
            id
        )
        .execute(&mut *tx)
        .await?;
        append_event(
            &mut tx,
            ctx,
            origin,
            NewEvent::new(&*self.clock, "tag", "deleted", id, &view)?,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get(&self, ctx: &HouseholdCtx, id: TagId) -> Result<TagView, AppError> {
        let mut conn = self.pool.acquire().await?;
        find(&mut conn, ctx, &id.to_string()).await
    }

    /// Tags ordered by name.
    pub async fn list(
        &self,
        ctx: &HouseholdCtx,
        page: PageRequest,
    ) -> Result<Page<TagView>, AppError> {
        let household_id = ctx.household_id.to_string();
        let (limit, offset) = (page.limit(), page.offset());
        let items = sqlx::query_as!(
            TagView,
            "SELECT id, name, description, created_at, updated_at FROM tags
             WHERE household_id = ? ORDER BY name LIMIT ? OFFSET ?",
            household_id,
            limit,
            offset,
        )
        .fetch_all(&self.pool)
        .await?;
        let total = sqlx::query_scalar!(
            "SELECT count(*) FROM tags WHERE household_id = ?",
            household_id
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(Page::new(items, page, total))
    }
}

async fn find(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    id: &str,
) -> Result<TagView, AppError> {
    let household_id = ctx.household_id.to_string();
    sqlx::query_as!(
        TagView,
        "SELECT id, name, description, created_at, updated_at FROM tags WHERE household_id = ? AND id = ?",
        household_id, id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// The IDs among `ids_json` (a JSON array) that exist in the household.
pub(crate) async fn existing(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    ids_json: &str,
) -> Result<Vec<String>, AppError> {
    let household_id = ctx.household_id.to_string();
    let ids = sqlx::query_scalar!(
        "SELECT id FROM tags WHERE household_id = ? AND id IN (SELECT value FROM json_each(?))",
        household_id,
        ids_json,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(ids)
}
