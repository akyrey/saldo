//! Budgets: a name, an optional automatic limit per period, and limits
//! (an amount between two dates) that withdrawals are measured against.

mod auto;
mod limits;
pub(crate) mod repo;
mod view;

use saldo_core::{BudgetId, Money, Period};
use sqlx::{SqliteConnection, SqlitePool};

use crate::clock::{SharedClock, format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::currencies;
use crate::error::AppError;
use crate::events::{NewEvent, append_event};
use crate::identity::{self, required};
use crate::page::{Page, PageRequest};
use repo::BudgetRow;

use limits::limit_views;
pub use limits::{BudgetLimitUpdate, NewBudgetLimit};
pub use view::{AutoBudgetView, BudgetLimitView, BudgetView};

/// An automatic limit as the client sends it. `currency` defaults to the
/// household's primary currency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoBudgetInput {
    /// `reset` or `rollover`.
    pub auto_type: String,
    /// `daily`, `weekly`, `monthly`, `quarterly`, `half_year` or `yearly`.
    pub period: String,
    pub amount: String,
    pub currency: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewBudget {
    pub name: String,
    pub active: bool,
    pub sort_order: i64,
    pub notes: Option<String>,
    pub auto_budget: Option<AutoBudgetInput>,
}

// `Default` written by hand because a new budget is active, and the derived
// default for `bool` is `false`.
impl Default for NewBudget {
    fn default() -> Self {
        Self {
            name: String::new(),
            active: true,
            sort_order: 0,
            notes: None,
            auto_budget: None,
        }
    }
}

/// `None` keeps a value; `notes: Some(None)` clears the notes and
/// `auto_budget: Some(None)` removes the automatic limit.
#[derive(Debug, Clone, Default)]
pub struct BudgetUpdate {
    pub name: Option<String>,
    pub active: Option<bool>,
    pub sort_order: Option<i64>,
    pub notes: Option<Option<String>>,
    pub auto_budget: Option<Option<AutoBudgetInput>>,
}

/// Filters for [`BudgetService::list`].
#[derive(Debug, Clone, Default)]
pub struct BudgetFilter {
    pub active: Option<bool>,
}

const DUPLICATE: &str = "a budget with this name exists";

#[derive(Clone)]
pub struct BudgetService {
    pool: SqlitePool,
    clock: SharedClock,
}

impl BudgetService {
    pub fn new(pool: SqlitePool, clock: SharedClock) -> Self {
        Self { pool, clock }
    }

    pub async fn create(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewBudget,
    ) -> Result<BudgetView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let result = self.create_in(&mut tx, ctx, origin, input).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Like [`Self::create`], inside the caller's transaction (used by imports).
    /// Outside imports, an automatic limit creates the current period's limit.
    pub async fn create_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewBudget,
    ) -> Result<BudgetView, AppError> {
        let now = format_timestamp(self.clock.now());
        let (auto_type, auto_period, auto_amount_minor, auto_currency_code) =
            auto_columns(conn, ctx, input.auto_budget.as_ref()).await?;
        let row = BudgetRow {
            id: new_id(&*self.clock).to_string(),
            name: required("name", &input.name)?,
            active: input.active,
            sort_order: input.sort_order,
            notes: clean_notes(input.notes),
            auto_type,
            auto_period,
            auto_amount_minor,
            auto_currency_code,
            created_at: now.clone(),
            updated_at: now,
        };
        let household_id = ctx.household_id.to_string();
        repo::insert(conn, &row, &household_id)
            .await
            .map_err(|e| AppError::conflict_on_unique(e, DUPLICATE))?;
        let view = view::budget_view(conn, row.clone()).await?;
        append_event(
            &mut *conn,
            ctx,
            origin,
            NewEvent::new(&*self.clock, "budget", "created", row.id.clone(), &view)?,
        )
        .await?;
        if origin != Origin::Import {
            self.auto_limits_for(conn, ctx, origin, &row).await?;
        }
        Ok(view)
    }

    pub async fn update(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: BudgetId,
        update: BudgetUpdate,
    ) -> Result<BudgetView, AppError> {
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
        id: BudgetId,
        update: BudgetUpdate,
    ) -> Result<BudgetView, AppError> {
        let household_id = ctx.household_id.to_string();
        let current = repo::find(conn, &household_id, &id.to_string()).await?;
        let mut row = BudgetRow {
            name: match &update.name {
                Some(name) => required("name", name)?,
                None => current.name.clone(),
            },
            active: update.active.unwrap_or(current.active),
            sort_order: update.sort_order.unwrap_or(current.sort_order),
            notes: update.notes.map_or(current.notes.clone(), clean_notes),
            updated_at: format_timestamp(self.clock.now()),
            ..current.clone()
        };
        if let Some(auto) = &update.auto_budget {
            let (auto_type, auto_period, minor, code) =
                auto_columns(conn, ctx, auto.as_ref()).await?;
            row = BudgetRow {
                auto_type,
                auto_period,
                auto_amount_minor: minor,
                auto_currency_code: code,
                ..row
            };
        }
        repo::update(conn, &row, &household_id)
            .await
            .map_err(|e| AppError::conflict_on_unique(e, DUPLICATE))?;
        let view = view::budget_view(conn, row.clone()).await?;
        append_event(
            &mut *conn,
            ctx,
            origin,
            NewEvent::new(&*self.clock, "budget", "updated", row.id.clone(), &view)?,
        )
        .await?;
        if origin != Origin::Import {
            self.auto_limits_for(conn, ctx, origin, &row).await?;
        }
        Ok(view)
    }

    /// Deletes the budget and its limits; splits that used it keep no budget.
    pub async fn delete(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: BudgetId,
    ) -> Result<(), AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
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
        id: BudgetId,
    ) -> Result<(), AppError> {
        let (household_id, id) = (ctx.household_id.to_string(), id.to_string());
        let row = repo::find(conn, &household_id, &id).await?;
        let view = view::budget_view(conn, row).await?;
        repo::delete(conn, &household_id, &id).await?;
        append_event(
            &mut *conn,
            ctx,
            origin,
            NewEvent::new(&*self.clock, "budget", "deleted", id, &view)?,
        )
        .await?;
        Ok(())
    }

    pub async fn get(&self, ctx: &HouseholdCtx, id: BudgetId) -> Result<BudgetView, AppError> {
        let mut conn = self.pool.acquire().await?;
        self.get_in(&mut conn, ctx, id).await
    }

    /// Like [`Self::get`], inside the caller's transaction.
    pub async fn get_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        id: BudgetId,
    ) -> Result<BudgetView, AppError> {
        let row = repo::find(conn, &ctx.household_id.to_string(), &id.to_string()).await?;
        view::budget_view(conn, row).await
    }

    /// Budgets ordered by `sort_order`, then name.
    pub async fn list(
        &self,
        ctx: &HouseholdCtx,
        filter: &BudgetFilter,
        page: PageRequest,
    ) -> Result<Page<BudgetView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        let household_id = ctx.household_id.to_string();
        let (rows, total) = repo::list(
            &mut conn,
            &household_id,
            filter.active,
            page.limit(),
            page.offset(),
        )
        .await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            items.push(view::budget_view(&mut conn, row).await?);
        }
        Ok(Page::new(items, page, total))
    }
}

/// The four `auto_*` columns for an optional automatic limit, checked.
async fn auto_columns(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    input: Option<&AutoBudgetInput>,
) -> Result<(Option<String>, Option<String>, Option<i64>, Option<String>), AppError> {
    let Some(input) = input else {
        return Ok((None, None, None, None));
    };
    let auto_type = saldo_core::AutoBudgetType::parse(&input.auto_type)
        .ok_or_else(|| AppError::invalid("auto_budget.type", "must be reset or rollover"))?;
    let period = Period::parse(&input.period).ok_or_else(|| {
        AppError::invalid(
            "auto_budget.period",
            "must be daily, weekly, monthly, quarterly, half_year or yearly",
        )
    })?;
    let code = match &input.currency {
        Some(code) => code.clone(),
        None => identity::primary_currency(conn, ctx).await?,
    };
    let currency = currencies::find(conn, &code, "auto_budget.currency").await?;
    let amount = positive_amount("auto_budget.amount", &input.amount, &currency)?;
    Ok((
        Some(auto_type.as_str().to_string()),
        Some(period.as_str().to_string()),
        Some(amount.minor()),
        Some(code),
    ))
}

/// Parses a decimal amount that must be greater than zero.
pub(crate) fn positive_amount(
    field: &str,
    text: &str,
    currency: &saldo_core::Currency,
) -> Result<Money, AppError> {
    let amount = Money::parse_decimal(text, currency)
        .map_err(|e| AppError::invalid(field, e.to_string()))?;
    if !amount.is_positive() {
        return Err(AppError::invalid(field, "must be greater than zero"));
    }
    Ok(amount)
}

/// Blank notes are stored as none.
fn clean_notes(notes: Option<String>) -> Option<String> {
    notes
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
}

/// The IDs among `ids_json` (a JSON array) that are budgets of the household
/// (for transaction validation).
pub(crate) async fn existing(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    ids_json: &str,
) -> Result<Vec<String>, AppError> {
    repo::existing(conn, &ctx.household_id.to_string(), ids_json).await
}

/// Every budget of the household and all their limits, for the export.
pub(crate) async fn all_in(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<(Vec<BudgetView>, Vec<BudgetLimitView>), AppError> {
    let household_id = ctx.household_id.to_string();
    let rows = repo::all(&mut *conn, &household_id).await?;
    let mut budgets = Vec::with_capacity(rows.len());
    let mut limits = Vec::new();
    for row in rows {
        let limit_rows = repo::limits(&mut *conn, &household_id, &row.id, None, None).await?;
        limits.extend(limit_views(&mut *conn, ctx, &row.id, limit_rows).await?);
        budgets.push(view::budget_view(&mut *conn, row).await?);
    }
    Ok((budgets, limits))
}
