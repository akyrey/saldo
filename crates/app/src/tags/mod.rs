//! Tags: a name per household, optionally with a description. Names are
//! unique ignoring case; [`merge`] folds duplicates left from older data.

mod duplicates;
mod merge;

pub use duplicates::{TagDuplicateGroup, TagUsageView, name_key};
pub use merge::TagMerge;

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

const DUPLICATE: &str = "a tag with this name exists (ignoring case)";

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
        let mut tx = crate::db::begin(&self.pool).await?;
        let result = self.create_in(&mut tx, ctx, origin, input).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Like [`Self::create`], inside the caller's transaction (used by imports).
    pub async fn create_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewTag,
    ) -> Result<TagView, AppError> {
        let name = required("name", &input.name)?;
        ensure_name_free(&mut *conn, ctx, &name, None).await?;
        let (id, household_id) = (
            new_id(&*self.clock).to_string(),
            ctx.household_id.to_string(),
        );
        let now = format_timestamp(self.clock.now());
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
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::conflict_on_unique(e, DUPLICATE))?;
        let view = find(&mut *conn, ctx, &id).await?;
        append_event(
            &mut *conn,
            ctx,
            origin,
            NewEvent::new(&*self.clock, "tag", "created", id, &view)?,
        )
        .await?;
        Ok(view)
    }

    pub async fn update(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: TagId,
        update: TagUpdate,
    ) -> Result<TagView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let result = self.update_in(&mut tx, ctx, origin, id, update).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Like [`Self::update`], inside the caller's transaction (used by imports).
    pub async fn update_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: TagId,
        update: TagUpdate,
    ) -> Result<TagView, AppError> {
        let id = id.to_string();
        let current = find(&mut *conn, ctx, &id).await?;
        let name = match update.name {
            Some(name) => required("name", &name)?,
            None => current.name.clone(),
        };
        // Only a new name is checked, so old case duplicates stay editable.
        if name != current.name {
            ensure_name_free(&mut *conn, ctx, &name, Some(&id)).await?;
        }
        let description = update.description.unwrap_or(current.description);
        let (household_id, now) = (
            ctx.household_id.to_string(),
            format_timestamp(self.clock.now()),
        );
        sqlx::query!(
            "UPDATE tags SET name = ?, description = ?, updated_at = ? WHERE household_id = ? AND id = ?",
            name, description, now, household_id, id,
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::conflict_on_unique(e, DUPLICATE))?;
        let view = find(&mut *conn, ctx, &id).await?;
        append_event(
            &mut *conn,
            ctx,
            origin,
            NewEvent::new(&*self.clock, "tag", "updated", id, &view)?,
        )
        .await?;
        Ok(view)
    }

    /// Deletes the tag and removes it from every split.
    pub async fn delete(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: TagId,
    ) -> Result<(), AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        self.delete_in(&mut tx, ctx, origin, id).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Like [`Self::delete`], inside the caller's transaction (used by imports).
    pub async fn delete_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: TagId,
    ) -> Result<(), AppError> {
        let id = id.to_string();
        let view = find(&mut *conn, ctx, &id).await?;
        let household_id = ctx.household_id.to_string();
        sqlx::query!(
            "DELETE FROM tags WHERE household_id = ? AND id = ?",
            household_id,
            id
        )
        .execute(&mut *conn)
        .await?;
        append_event(
            &mut *conn,
            ctx,
            origin,
            NewEvent::new(&*self.clock, "tag", "deleted", id, &view)?,
        )
        .await?;
        Ok(())
    }

    pub async fn get(&self, ctx: &HouseholdCtx, id: TagId) -> Result<TagView, AppError> {
        let mut conn = self.pool.acquire().await?;
        self.get_in(&mut conn, ctx, id).await
    }

    /// Like [`Self::get`], inside the caller's transaction.
    pub async fn get_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        id: TagId,
    ) -> Result<TagView, AppError> {
        find(conn, ctx, &id.to_string()).await
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

pub(crate) async fn find(
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

/// Fails with `Conflict` when another tag (not `except`) has `name`, ignoring case.
async fn ensure_name_free(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    name: &str,
    except: Option<&str>,
) -> Result<(), AppError> {
    let key = name_key(name);
    let taken = all_in(conn, ctx)
        .await?
        .iter()
        .any(|t| name_key(&t.name) == key && Some(t.id.as_str()) != except);
    if taken {
        return Err(AppError::Conflict(DUPLICATE.to_string()));
    }
    Ok(())
}

/// The tag named `name`, ignoring case and surrounding spaces (the first by
/// name if old duplicates exist).
pub async fn find_by_name_ignoring_case(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    name: &str,
) -> Result<Option<TagView>, AppError> {
    let key = name_key(name);
    let tags = all_in(conn, ctx).await?;
    Ok(tags.into_iter().find(|t| name_key(&t.name) == key))
}

/// Every tag of the household as lowercase name → ID (Firefly refers to tags
/// by name; look up with [`name_key`]).
pub async fn ids_by_name(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<std::collections::HashMap<String, String>, AppError> {
    let household_id = ctx.household_id.to_string();
    let rows = sqlx::query!(
        "SELECT id, name FROM tags WHERE household_id = ?",
        household_id
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (name_key(&r.name), r.id))
        .collect())
}

/// Every tag of the household (for export).
pub async fn all_in(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<Vec<TagView>, AppError> {
    let household_id = ctx.household_id.to_string();
    let rows = sqlx::query_as!(
        TagView,
        "SELECT id, name, description, created_at, updated_at FROM tags WHERE household_id = ? ORDER BY name",
        household_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}
