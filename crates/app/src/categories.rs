//! Categories: a name per household, optionally with notes.

use saldo_core::CategoryId;
use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};

use crate::clock::{SharedClock, format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::AppError;
use crate::events::{NewEvent, append_event};
use crate::identity::required;
use crate::page::{Page, PageRequest};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CategoryView {
    pub id: String,
    pub name: String,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct NewCategory {
    pub name: String,
    pub notes: Option<String>,
}

/// `None` keeps a value; `notes: Some(None)` clears the notes.
#[derive(Debug, Clone, Default)]
pub struct CategoryUpdate {
    pub name: Option<String>,
    pub notes: Option<Option<String>>,
}

const DUPLICATE: &str = "a category with this name exists";

#[derive(Clone)]
pub struct CategoryService {
    pool: SqlitePool,
    clock: SharedClock,
}

impl CategoryService {
    pub fn new(pool: SqlitePool, clock: SharedClock) -> Self {
        Self { pool, clock }
    }

    pub async fn create(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewCategory,
    ) -> Result<CategoryView, AppError> {
        let mut tx = self.pool.begin().await?;
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
        input: NewCategory,
    ) -> Result<CategoryView, AppError> {
        let name = required("name", &input.name)?;
        let (id, household_id) = (
            new_id(&*self.clock).to_string(),
            ctx.household_id.to_string(),
        );
        let now = format_timestamp(self.clock.now());
        sqlx::query!(
            "INSERT INTO categories (id, household_id, name, notes, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            id,
            household_id,
            name,
            input.notes,
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
            NewEvent::new(&*self.clock, "category", "created", id, &view)?,
        )
        .await?;
        Ok(view)
    }

    pub async fn update(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: CategoryId,
        update: CategoryUpdate,
    ) -> Result<CategoryView, AppError> {
        let mut tx = self.pool.begin().await?;
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
        id: CategoryId,
        update: CategoryUpdate,
    ) -> Result<CategoryView, AppError> {
        let id = id.to_string();
        let current = find(&mut *conn, ctx, &id).await?;
        let name = match update.name {
            Some(name) => required("name", &name)?,
            None => current.name,
        };
        let notes = update.notes.unwrap_or(current.notes);
        let (household_id, now) = (
            ctx.household_id.to_string(),
            format_timestamp(self.clock.now()),
        );
        sqlx::query!(
            "UPDATE categories SET name = ?, notes = ?, updated_at = ? WHERE household_id = ? AND id = ?",
            name, notes, now, household_id, id,
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::conflict_on_unique(e, DUPLICATE))?;
        let view = find(&mut *conn, ctx, &id).await?;
        append_event(
            &mut *conn,
            ctx,
            origin,
            NewEvent::new(&*self.clock, "category", "updated", id, &view)?,
        )
        .await?;
        Ok(view)
    }

    /// Deletes the category; splits that used it keep no category.
    pub async fn delete(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: CategoryId,
    ) -> Result<(), AppError> {
        let mut tx = self.pool.begin().await?;
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
        id: CategoryId,
    ) -> Result<(), AppError> {
        let id = id.to_string();
        let view = find(&mut *conn, ctx, &id).await?;
        let household_id = ctx.household_id.to_string();
        sqlx::query!(
            "DELETE FROM categories WHERE household_id = ? AND id = ?",
            household_id,
            id
        )
        .execute(&mut *conn)
        .await?;
        append_event(
            &mut *conn,
            ctx,
            origin,
            NewEvent::new(&*self.clock, "category", "deleted", id, &view)?,
        )
        .await?;
        Ok(())
    }

    pub async fn get(&self, ctx: &HouseholdCtx, id: CategoryId) -> Result<CategoryView, AppError> {
        let mut conn = self.pool.acquire().await?;
        self.get_in(&mut conn, ctx, id).await
    }

    /// Like [`Self::get`], inside the caller's transaction.
    pub async fn get_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        id: CategoryId,
    ) -> Result<CategoryView, AppError> {
        find(conn, ctx, &id.to_string()).await
    }

    /// Categories ordered by name.
    pub async fn list(
        &self,
        ctx: &HouseholdCtx,
        page: PageRequest,
    ) -> Result<Page<CategoryView>, AppError> {
        let household_id = ctx.household_id.to_string();
        let (limit, offset) = (page.limit(), page.offset());
        let items = sqlx::query_as!(
            CategoryView,
            "SELECT id, name, notes, created_at, updated_at FROM categories
             WHERE household_id = ? ORDER BY name LIMIT ? OFFSET ?",
            household_id,
            limit,
            offset,
        )
        .fetch_all(&self.pool)
        .await?;
        let total = sqlx::query_scalar!(
            "SELECT count(*) FROM categories WHERE household_id = ?",
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
) -> Result<CategoryView, AppError> {
    let household_id = ctx.household_id.to_string();
    sqlx::query_as!(
        CategoryView,
        "SELECT id, name, notes, created_at, updated_at FROM categories WHERE household_id = ? AND id = ?",
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
        "SELECT id FROM categories WHERE household_id = ? AND id IN (SELECT value FROM json_each(?))",
        household_id, ids_json,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(ids)
}
