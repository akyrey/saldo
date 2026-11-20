//! Saved views: named filters for a list page, shared by the household
//! (Phase 10). The server stores the filters as given; the client that
//! wrote them reads them back and validates them like any URL.

use serde::Serialize;
use serde_json::{Map, Value};
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

use crate::clock::{SharedClock, format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::{AppError, parse_stored};
use crate::events::{NewEvent, append_event};
use crate::identity::required;

/// The list pages that can have saved views.
pub const PAGES: [&str; 1] = ["transactions"];
/// The most filter keys one view may store.
pub const MAX_FILTER_KEYS: usize = 30;
/// The most bytes the filters may take as JSON.
pub const MAX_FILTER_BYTES: usize = 2048;

const ENTITY: &str = "saved_view";
const DUPLICATE: &str = "a saved view with this name exists on this page";

#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct SavedViewView {
    pub id: String,
    pub name: String,
    pub page: String,
    /// The page's filters, as the client stored them.
    #[schema(value_type = Object)]
    pub filters: Value,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct NewSavedView {
    pub name: String,
    pub page: String,
    pub filters: Value,
    pub sort_order: i64,
}

/// `None` keeps a value.
#[derive(Debug, Clone, Default)]
pub struct SavedViewUpdate {
    pub name: Option<String>,
    pub filters: Option<Value>,
    pub sort_order: Option<i64>,
}

#[derive(Clone)]
pub struct SavedViewService {
    pool: SqlitePool,
    clock: SharedClock,
}

impl SavedViewService {
    pub fn new(pool: SqlitePool, clock: SharedClock) -> Self {
        Self { pool, clock }
    }

    pub async fn create(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewSavedView,
    ) -> Result<SavedViewView, AppError> {
        let name = required("name", &input.name)?;
        let page = check_page(&input.page)?;
        let filters = check_filters(&input.filters)?;
        let id = new_id(&*self.clock).to_string();
        let household_id = ctx.household_id.to_string();
        let now = format_timestamp(self.clock.now());
        let mut tx = crate::db::begin(&self.pool).await?;
        sqlx::query!(
            "INSERT INTO saved_views (id, household_id, name, page, filters, sort_order, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            id, household_id, name, page, filters, input.sort_order, now, now,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::conflict_on_unique(e, DUPLICATE))?;
        let view = self.emit(&mut tx, ctx, origin, "created", &id).await?;
        tx.commit().await?;
        Ok(view)
    }

    pub async fn update(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: Uuid,
        update: SavedViewUpdate,
    ) -> Result<SavedViewView, AppError> {
        let id = id.to_string();
        let mut tx = crate::db::begin(&self.pool).await?;
        let current = find(&mut tx, ctx, &id).await?;
        let name = match update.name {
            Some(name) => required("name", &name)?,
            None => current.name,
        };
        let filters = check_filters(update.filters.as_ref().unwrap_or(&current.filters))?;
        let sort_order = update.sort_order.unwrap_or(current.sort_order);
        let household_id = ctx.household_id.to_string();
        let now = format_timestamp(self.clock.now());
        sqlx::query!(
            "UPDATE saved_views SET name = ?, filters = ?, sort_order = ?, updated_at = ?
             WHERE household_id = ? AND id = ?",
            name,
            filters,
            sort_order,
            now,
            household_id,
            id,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::conflict_on_unique(e, DUPLICATE))?;
        let view = self.emit(&mut tx, ctx, origin, "updated", &id).await?;
        tx.commit().await?;
        Ok(view)
    }

    pub async fn delete(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: Uuid,
    ) -> Result<(), AppError> {
        let id = id.to_string();
        let mut tx = crate::db::begin(&self.pool).await?;
        let view = find(&mut tx, ctx, &id).await?;
        let household_id = ctx.household_id.to_string();
        sqlx::query!(
            "DELETE FROM saved_views WHERE household_id = ? AND id = ?",
            household_id,
            id
        )
        .execute(&mut *tx)
        .await?;
        let event = NewEvent::new(&*self.clock, ENTITY, "deleted", id, &view)?;
        append_event(&mut tx, ctx, origin, event).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get(&self, ctx: &HouseholdCtx, id: Uuid) -> Result<SavedViewView, AppError> {
        let mut conn = self.pool.acquire().await?;
        find(&mut conn, ctx, &id.to_string()).await
    }

    /// The page's views, by `sort_order`, then name. Unpaginated: a
    /// household keeps a handful.
    pub async fn list(
        &self,
        ctx: &HouseholdCtx,
        page: &str,
    ) -> Result<Vec<SavedViewView>, AppError> {
        let page = check_page(page)?;
        let household_id = ctx.household_id.to_string();
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, name, page, filters, sort_order, created_at, updated_at FROM saved_views
             WHERE household_id = ? AND page = ? ORDER BY sort_order, name",
            household_id,
            page,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Row::view).collect()
    }

    /// Reads the stored view back and appends its event.
    async fn emit(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        action: &str,
        id: &str,
    ) -> Result<SavedViewView, AppError> {
        let view = find(&mut *conn, ctx, id).await?;
        let event = NewEvent::new(&*self.clock, ENTITY, action, id.to_string(), &view)?;
        append_event(&mut *conn, ctx, origin, event).await?;
        Ok(view)
    }
}

/// Every saved view of the household (for export), by page, then order.
pub(crate) async fn all_in(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<Vec<SavedViewView>, AppError> {
    let household_id = ctx.household_id.to_string();
    let rows = sqlx::query_as!(
        Row,
        "SELECT id, name, page, filters, sort_order, created_at, updated_at FROM saved_views
         WHERE household_id = ? ORDER BY page, sort_order, name",
        household_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    rows.into_iter().map(Row::view).collect()
}

struct Row {
    id: String,
    name: String,
    page: String,
    filters: String,
    sort_order: i64,
    created_at: String,
    updated_at: String,
}

impl Row {
    fn view(self) -> Result<SavedViewView, AppError> {
        Ok(SavedViewView {
            filters: parse_stored(&self.filters)?,
            id: self.id,
            name: self.name,
            page: self.page,
            sort_order: self.sort_order,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

async fn find(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    id: &str,
) -> Result<SavedViewView, AppError> {
    let household_id = ctx.household_id.to_string();
    sqlx::query_as!(
        Row,
        "SELECT id, name, page, filters, sort_order, created_at, updated_at FROM saved_views
         WHERE household_id = ? AND id = ?",
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?
    .view()
}

fn check_page(page: &str) -> Result<&'static str, AppError> {
    PAGES
        .iter()
        .find(|p| **p == page)
        .copied()
        .ok_or_else(|| AppError::invalid("page", format!("must be one of {}", PAGES.join(", "))))
}

/// The filters as stored text: a flat JSON object of strings, numbers and
/// booleans, small enough to fit a URL.
fn check_filters(filters: &Value) -> Result<String, AppError> {
    let invalid = |message: &str| AppError::invalid("filters", message);
    let Value::Object(map) = filters else {
        return Err(invalid("must be an object"));
    };
    if map.len() > MAX_FILTER_KEYS {
        return Err(invalid(&format!(
            "must have at most {MAX_FILTER_KEYS} keys"
        )));
    }
    if !map.values().all(is_flat) {
        return Err(invalid("values must be strings, numbers or booleans"));
    }
    let text = Value::Object(Map::clone(map)).to_string();
    if text.len() > MAX_FILTER_BYTES {
        return Err(invalid(&format!(
            "must be at most {MAX_FILTER_BYTES} bytes"
        )));
    }
    Ok(text)
}

fn is_flat(value: &Value) -> bool {
    matches!(value, Value::String(_) | Value::Number(_) | Value::Bool(_))
}
