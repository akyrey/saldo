//! Transaction templates: saved, partial transactions the web editor fills
//! itself from (Phase 8). There is no "instantiate": the editor posts a
//! normal transaction, so the ledger rules run when it is saved.

mod from_group;
mod input;
mod repo;
mod view;

use std::fmt;
use std::str::FromStr;

use saldo_core::IdError;
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

use crate::clock::{SharedClock, format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::AppError;
use crate::events::{NewEvent, append_event};
use crate::page::{Page, PageRequest};
use crate::transactions::TransactionService;

pub use input::{NewTemplate, TEMPLATE_KINDS, TemplateSplitInput, TemplateUpdate};
pub use view::{TemplateSplitView, TemplateView};

/// Identifies a template. Lives here, not in `saldo-core`, because the
/// domain logic never sees templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TemplateId(pub Uuid);

impl fmt::Display for TemplateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for TemplateId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(Self).map_err(|_| IdError {
            input: s.to_string(),
        })
    }
}

#[derive(Clone)]
pub struct TemplateService {
    pool: SqlitePool,
    clock: SharedClock,
    transactions: TransactionService,
}

impl TemplateService {
    pub fn new(pool: SqlitePool, clock: SharedClock, transactions: TransactionService) -> Self {
        Self {
            pool,
            clock,
            transactions,
        }
    }

    pub async fn create(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewTemplate,
    ) -> Result<TemplateView, AppError> {
        let mut tx = self.pool.begin().await?;
        let view = self.create_in(&mut tx, ctx, origin, input).await?;
        tx.commit().await?;
        Ok(view)
    }

    /// Like [`Self::create`], inside the caller's transaction.
    pub async fn create_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewTemplate,
    ) -> Result<TemplateView, AppError> {
        let (name, kind) = input::name_and_kind(&input.name, &input.kind)?;
        let splits = input::check_splits(conn, ctx, kind, input.splits).await?;
        let household_id = ctx.household_id.to_string();
        let sort_order = match input.sort_order {
            Some(order) => order,
            None => repo::next_sort_order(conn, &household_id).await?,
        };
        let title = input::text(input.title);
        let fields = repo::Fields {
            name: &name,
            kind: input::kind_name(kind),
            title: title.as_deref(),
            splits: &splits,
            sort_order,
        };
        let id = new_id(&*self.clock).to_string();
        let now = format_timestamp(self.clock.now());
        repo::insert(conn, &household_id, &id, &fields, &now).await?;
        self.written(conn, ctx, origin, "created", &id).await
    }

    pub async fn update(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: TemplateId,
        update: TemplateUpdate,
    ) -> Result<TemplateView, AppError> {
        let mut tx = self.pool.begin().await?;
        let view = self.update_in(&mut tx, ctx, origin, id, update).await?;
        tx.commit().await?;
        Ok(view)
    }

    /// Like [`Self::update`], inside the caller's transaction. Kept splits
    /// are checked again, since a new kind may not fit them.
    pub async fn update_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: TemplateId,
        update: TemplateUpdate,
    ) -> Result<TemplateView, AppError> {
        let id = id.to_string();
        let current = self.find(conn, ctx, &id).await?;
        let name = update.name.unwrap_or(current.name);
        let kind = update.kind.unwrap_or(current.kind);
        let (name, kind) = input::name_and_kind(&name, &kind)?;
        let splits = match update.splits {
            Some(splits) => splits,
            None => current
                .splits
                .iter()
                .map(input::input_of)
                .collect::<Result<_, _>>()?,
        };
        let splits = input::check_splits(conn, ctx, kind, splits).await?;
        let title = update.title.map_or(current.title, input::text);
        let fields = repo::Fields {
            name: &name,
            kind: input::kind_name(kind),
            title: title.as_deref(),
            splits: &splits,
            sort_order: update.sort_order.unwrap_or(current.sort_order),
        };
        let now = format_timestamp(self.clock.now());
        let household_id = ctx.household_id.to_string();
        repo::update(conn, &household_id, &id, &fields, &now).await?;
        self.written(conn, ctx, origin, "updated", &id).await
    }

    pub async fn delete(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: TemplateId,
    ) -> Result<(), AppError> {
        let mut tx = self.pool.begin().await?;
        self.delete_in(&mut tx, ctx, origin, id).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Like [`Self::delete`], inside the caller's transaction.
    pub async fn delete_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: TemplateId,
    ) -> Result<(), AppError> {
        let id = id.to_string();
        let view = self.find(conn, ctx, &id).await?;
        repo::delete(conn, &ctx.household_id.to_string(), &id).await?;
        let event = NewEvent::new(&*self.clock, "template", "deleted", id, &view)?;
        append_event(conn, ctx, origin, event).await?;
        Ok(())
    }

    pub async fn get(&self, ctx: &HouseholdCtx, id: TemplateId) -> Result<TemplateView, AppError> {
        let mut conn = self.pool.acquire().await?;
        self.find(&mut conn, ctx, &id.to_string()).await
    }

    /// Templates by sort order, then name.
    pub async fn list(
        &self,
        ctx: &HouseholdCtx,
        page: PageRequest,
    ) -> Result<Page<TemplateView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        let household_id = ctx.household_id.to_string();
        let rows = repo::list(&mut conn, &household_id, page.limit(), page.offset()).await?;
        let items = view::views_of(&mut conn, ctx, rows).await?;
        let total = repo::count(&mut conn, &household_id).await?;
        Ok(Page::new(items, page, total))
    }

    async fn find(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        id: &str,
    ) -> Result<TemplateView, AppError> {
        let row = repo::find(conn, &ctx.household_id.to_string(), id).await?;
        let mut views = view::views_of(conn, ctx, vec![row]).await?;
        views.pop().ok_or(AppError::NotFound)
    }

    /// Reads the written template back and appends `template.<verb>`.
    async fn written(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        verb: &str,
        id: &str,
    ) -> Result<TemplateView, AppError> {
        let view = self.find(conn, ctx, id).await?;
        let event = NewEvent::new(&*self.clock, "template", verb, id.to_string(), &view)?;
        append_event(conn, ctx, origin, event).await?;
        Ok(view)
    }
}

/// Every template of the household, in display order (for export).
pub async fn all_in(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<Vec<TemplateView>, AppError> {
    let rows = repo::list(conn, &ctx.household_id.to_string(), -1, 0).await?;
    view::views_of(conn, ctx, rows).await
}
