//! Transaction groups: always written as a whole (create, replace, delete).

mod prepare;
mod repo;
mod view;

use chrono::NaiveDate;
use saldo_core::{AccountId, BillId, BudgetId, CategoryId, GroupId, TagId};
use serde_json::{Map, Value};
use sqlx::{SqliteConnection, SqlitePool};

use crate::clock::{SharedClock, format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::AppError;
use crate::events::{NewEvent, append_event};
use crate::page::{Page, PageRequest};
use crate::scope::Scope;

pub use view::{SplitView, TransactionGroupView};

pub const KINDS: [&str; 5] = [
    "withdrawal",
    "deposit",
    "transfer",
    "opening_balance",
    "reconciliation",
];

/// A group as the client sends it.
#[derive(Debug, Clone, Default)]
pub struct TransactionInput {
    pub title: Option<String>,
    pub splits: Vec<SplitInput>,
}

/// One split as the client sends it. `currency` defaults to the currency of
/// the asset or liability side.
#[derive(Debug, Clone)]
pub struct SplitInput {
    pub date: NaiveDate,
    pub description: String,
    pub source_id: AccountId,
    pub destination_id: AccountId,
    pub amount: String,
    pub currency: Option<String>,
    pub foreign_amount: Option<String>,
    pub foreign_currency: Option<String>,
    pub category_id: Option<CategoryId>,
    pub budget_id: Option<BudgetId>,
    pub bill_id: Option<BillId>,
    pub tag_ids: Vec<TagId>,
    pub reconciled: bool,
    pub notes: Option<String>,
    pub meta: Map<String, Value>,
}

/// Filters for listing groups; a group matches if any split matches.
#[derive(Debug, Clone, Default)]
pub struct TransactionFilter {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub account_id: Option<AccountId>,
    pub category_id: Option<CategoryId>,
    pub tag_id: Option<TagId>,
    pub budget_id: Option<BudgetId>,
    pub bill_id: Option<BillId>,
    pub kind: Option<String>,
    /// Text searched in split descriptions and the group title.
    pub q: Option<String>,
    pub scope: Scope,
}

#[derive(Clone)]
pub struct TransactionService {
    pool: SqlitePool,
    clock: SharedClock,
}

impl TransactionService {
    pub fn new(pool: SqlitePool, clock: SharedClock) -> Self {
        Self { pool, clock }
    }

    /// Validates through `saldo-core`, writes the group, its splits and tags,
    /// and appends `transaction.created`, all in one database transaction.
    pub async fn create(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: TransactionInput,
    ) -> Result<TransactionGroupView, AppError> {
        let mut tx = self.pool.begin().await?;
        let group = prepare::validate(&mut tx, ctx, input).await?;
        let household_id = ctx.household_id.to_string();
        let id = new_id(&*self.clock).to_string();
        let now = format_timestamp(self.clock.now());
        let user_id = ctx.user_id.to_string();
        repo::insert_group(&mut tx, &household_id, &id, group.title(), &user_id, &now).await?;
        repo::insert_splits(&mut tx, &*self.clock, &household_id, &id, &group, &now).await?;
        let view = load_one(&mut tx, ctx, &id).await?;
        let event = NewEvent::new(&*self.clock, "transaction", "created", id, &view)?;
        append_event(&mut tx, ctx, origin, event).await?;
        tx.commit().await?;
        Ok(view)
    }

    /// Replaces the title and all splits of a group (the splits get new IDs).
    pub async fn replace(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: GroupId,
        input: TransactionInput,
    ) -> Result<TransactionGroupView, AppError> {
        let (household_id, id) = (ctx.household_id.to_string(), id.to_string());
        let mut tx = self.pool.begin().await?;
        if !repo::group_exists(&mut tx, &household_id, &id).await? {
            return Err(AppError::NotFound);
        }
        let group = prepare::validate(&mut tx, ctx, input).await?;
        let now = format_timestamp(self.clock.now());
        repo::update_group(&mut tx, &household_id, &id, group.title(), &now).await?;
        repo::delete_splits(&mut tx, &household_id, &id).await?;
        repo::insert_splits(&mut tx, &*self.clock, &household_id, &id, &group, &now).await?;
        let view = load_one(&mut tx, ctx, &id).await?;
        let event = NewEvent::new(&*self.clock, "transaction", "updated", id, &view)?;
        append_event(&mut tx, ctx, origin, event).await?;
        tx.commit().await?;
        Ok(view)
    }

    /// Deletes a group and its splits; the event carries its last state.
    pub async fn delete(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: GroupId,
    ) -> Result<(), AppError> {
        let id = id.to_string();
        let mut tx = self.pool.begin().await?;
        let view = load_one(&mut tx, ctx, &id).await?;
        repo::delete_group(&mut tx, &ctx.household_id.to_string(), &id).await?;
        let event = NewEvent::new(&*self.clock, "transaction", "deleted", id, &view)?;
        append_event(&mut tx, ctx, origin, event).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get(
        &self,
        ctx: &HouseholdCtx,
        id: GroupId,
    ) -> Result<TransactionGroupView, AppError> {
        let mut conn = self.pool.acquire().await?;
        load_one(&mut conn, ctx, &id.to_string()).await
    }

    /// Groups with a matching split, newest first.
    pub async fn list(
        &self,
        ctx: &HouseholdCtx,
        filter: &TransactionFilter,
        page: PageRequest,
    ) -> Result<Page<TransactionGroupView>, AppError> {
        if filter
            .kind
            .as_ref()
            .is_some_and(|k| !KINDS.contains(&k.as_str()))
        {
            return Err(AppError::invalid(
                "kind",
                format!("must be one of {}", KINDS.join(", ")),
            ));
        }
        let repo_filter = repo::ListFilter {
            from: filter.from.map(|d| d.to_string()),
            to: filter.to.map(|d| d.to_string()),
            account_id: filter.account_id.map(|id| id.to_string()),
            category_id: filter.category_id.map(|id| id.to_string()),
            tag_id: filter.tag_id.map(|id| id.to_string()),
            budget_id: filter.budget_id.map(|id| id.to_string()),
            bill_id: filter.bill_id.map(|id| id.to_string()),
            kind: filter.kind.clone(),
            q: filter.q.clone().filter(|q| !q.trim().is_empty()),
            owner_user_id: filter.scope.owner(ctx),
            limit: page.limit(),
            offset: page.offset(),
        };
        let household_id = ctx.household_id.to_string();
        let mut conn = self.pool.acquire().await?;
        let (ids, total) = repo::list_ids(&mut conn, &household_id, &repo_filter).await?;
        let items = view::load(&mut conn, &household_id, &ids).await?;
        Ok(Page::new(items, page, total))
    }
}

async fn load_one(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    id: &str,
) -> Result<TransactionGroupView, AppError> {
    let ids = [id.to_string()];
    view::load(conn, &ctx.household_id.to_string(), &ids)
        .await?
        .pop()
        .ok_or(AppError::NotFound)
}
