//! Transaction groups: always written as a whole (create, replace, delete).

mod input;
mod prepare;
pub(crate) mod recurring;
mod repo;
mod split_ids;
mod view;

use chrono::NaiveDate;
use saldo_core::{AccountId, BillId, BudgetId, CategoryId, GroupId, SplitId, TagId};
use serde_json::{Map, Value};
use sqlx::{SqliteConnection, SqlitePool};

use crate::attachments::delete_all_of_in;
use crate::bills;
use crate::clock::{SharedClock, format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::AppError;
use crate::events::{NewEvent, append_event};
use crate::page::{Page, PageRequest};
use crate::scope::Scope;

pub use input::input_from_view;
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
    /// On an update, the existing split this one continues: it keeps its ID
    /// and attachments. `None` makes a new split. Not allowed on create.
    pub id: Option<SplitId>,
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
        input: TransactionInput,
    ) -> Result<TransactionGroupView, AppError> {
        let view = self.write_create_in(&mut *conn, ctx, origin, input).await?;
        self.run_rules(conn, ctx, origin, "create", view).await
    }

    /// Creates the group without running the rules (a recurrence decides
    /// itself whether they run).
    pub(crate) async fn write_create_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: TransactionInput,
    ) -> Result<TransactionGroupView, AppError> {
        split_ids::reject_ids(&input)?;
        let group = prepare::validate(&mut *conn, ctx, origin, input).await?;
        let household_id = ctx.household_id.to_string();
        let id = new_id(&*self.clock).to_string();
        let now = format_timestamp(self.clock.now());
        let user_id = ctx.user_id.to_string();
        repo::insert_group(
            &mut *conn,
            &household_id,
            &id,
            group.title(),
            &user_id,
            &now,
        )
        .await?;
        repo::write_splits(
            &mut *conn,
            &*self.clock,
            &household_id,
            &id,
            &group,
            &[],
            &now,
        )
        .await?;
        let view = load_one(&mut *conn, ctx, &id).await?;
        let event = NewEvent::new(&*self.clock, "transaction", "created", id, &view)?;
        append_event(&mut *conn, ctx, origin, event).await?;
        // Splits with a bill pay that bill's matching period (`bill.paid`).
        bills::link_splits_in(&mut *conn, &*self.clock, ctx, origin, &view).await?;
        Ok(view)
    }

    /// Replaces the title and all splits of a group. A split that names one
    /// of the group's split IDs keeps it (and its attachments); the others
    /// are new, and the group's splits left out are deleted with their
    /// attachments.
    pub async fn replace(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: GroupId,
        input: TransactionInput,
    ) -> Result<TransactionGroupView, AppError> {
        let mut tx = self.pool.begin().await?;
        let result = self.replace_in(&mut tx, ctx, origin, id, input).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Like [`Self::replace`], inside the caller's transaction (used by imports).
    pub async fn replace_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: GroupId,
        input: TransactionInput,
    ) -> Result<TransactionGroupView, AppError> {
        let view = self.write_replace_in(conn, ctx, origin, id, input).await?;
        self.run_rules(conn, ctx, origin, "update", view).await
    }

    /// Runs the household's rules for `trigger_on` on a just-saved group and
    /// saves their changes with `Origin::Rule`. Imports skip rules (Firefly
    /// already applied its own), and changes made by rules do not run rules
    /// again. A rule whose change breaks a ledger rule is logged and ignored,
    /// so a misconfigured rule never blocks entering a transaction.
    async fn run_rules(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        trigger_on: &str,
        view: TransactionGroupView,
    ) -> Result<TransactionGroupView, AppError> {
        if matches!(origin, Origin::Import | Origin::Rule) {
            return Ok(view);
        }
        let rules = crate::rules::engine::load(&mut *conn, ctx, trigger_on).await?;
        let Some(changed) = crate::rules::engine::changes(&mut *conn, &rules, &view).await? else {
            return Ok(view);
        };
        let id: GroupId = view
            .id
            .parse()
            .map_err(|_| AppError::Internal(format!("bad group id {}", view.id)))?;
        match self
            .write_replace_in(conn, ctx, Origin::Rule, id, changed)
            .await
        {
            Ok(updated) => Ok(updated),
            Err(AppError::Validation(errors)) => {
                let codes: Vec<_> = errors.iter().map(|e| e.code()).collect();
                tracing::warn!(group = %view.id, ?codes, "rule changes break a ledger rule; not applied");
                Ok(view)
            }
            Err(other) => Err(other),
        }
    }

    /// Replaces a group without running rules (used by rules themselves).
    pub(crate) async fn write_replace_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: GroupId,
        input: TransactionInput,
    ) -> Result<TransactionGroupView, AppError> {
        let (household_id, id) = (ctx.household_id.to_string(), id.to_string());
        if !repo::group_exists(&mut *conn, &household_id, &id).await? {
            return Err(AppError::NotFound);
        }
        let plan = split_ids::plan(&mut *conn, &household_id, &id, &input).await?;
        let group = prepare::validate(&mut *conn, ctx, origin, input).await?;
        let now = format_timestamp(self.clock.now());
        repo::update_group(&mut *conn, &household_id, &id, group.title(), &now).await?;
        // Periods the old splits paid open again, then the saved splits may
        // pay them once more.
        bills::release_group_in(&mut *conn, &*self.clock, ctx, origin, &id).await?;
        for dropped in &plan.dropped {
            repo::delete_split(&mut *conn, &household_id, dropped).await?;
        }
        delete_all_of_in(
            &mut *conn,
            &*self.clock,
            ctx,
            origin,
            "split",
            &plan.dropped,
        )
        .await?;
        repo::park_positions(&mut *conn, &household_id, &id).await?;
        let kept = &plan.kept;
        repo::write_splits(
            &mut *conn,
            &*self.clock,
            &household_id,
            &id,
            &group,
            kept,
            &now,
        )
        .await?;
        let view = load_one(&mut *conn, ctx, &id).await?;
        let event = NewEvent::new(&*self.clock, "transaction", "updated", id, &view)?;
        append_event(&mut *conn, ctx, origin, event).await?;
        bills::link_splits_in(&mut *conn, &*self.clock, ctx, origin, &view).await?;
        Ok(view)
    }

    /// Deletes a group, its splits and their attachments; the event carries
    /// its last state.
    pub async fn delete(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: GroupId,
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
        id: GroupId,
    ) -> Result<(), AppError> {
        let id = id.to_string();
        let view = load_one(&mut *conn, ctx, &id).await?;
        bills::release_group_in(&mut *conn, &*self.clock, ctx, origin, &id).await?;
        repo::delete_group(&mut *conn, &ctx.household_id.to_string(), &id).await?;
        let split_ids: Vec<String> = view.splits.iter().map(|s| s.id.clone()).collect();
        delete_all_of_in(&mut *conn, &*self.clock, ctx, origin, "split", &split_ids).await?;
        let event = NewEvent::new(&*self.clock, "transaction", "deleted", id, &view)?;
        append_event(&mut *conn, ctx, origin, event).await?;
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

    /// Like [`Self::get`], inside the caller's transaction.
    pub async fn get_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        id: GroupId,
    ) -> Result<TransactionGroupView, AppError> {
        load_one(conn, ctx, &id.to_string()).await
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

/// Every group of the household, oldest first (for export), loaded in chunks.
pub async fn all_in(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<Vec<TransactionGroupView>, AppError> {
    const CHUNK: usize = 500;
    let household_id = ctx.household_id.to_string();
    let ids = sqlx::query_scalar!(
        r#"SELECT g.id AS "id!" FROM transaction_groups g WHERE g.household_id = ?
           ORDER BY (SELECT min(s.date) FROM splits s WHERE s.group_id = g.id), g.id"#,
        household_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut groups = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(CHUNK) {
        groups.extend(view::load(&mut *conn, &household_id, chunk).await?);
    }
    Ok(groups)
}
