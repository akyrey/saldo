//! Budget limits: an amount a budget may spend between two dates.

use std::collections::HashMap;

use chrono::NaiveDate;
use saldo_core::{BudgetId, Carry, Money, Spend};
use sqlx::SqliteConnection;

use super::repo::{self, LimitRow};
use super::view::{self, BudgetLimitView};
use super::{BudgetService, clean_notes, positive_amount};
use crate::clock::{format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::currencies;
use crate::error::{AppError, parse_stored};
use crate::events::{NewEvent, append_event};
use crate::identity;
use crate::page::{Page, PageRequest};

/// A limit as the client sends it; `currency` defaults to the household's
/// primary currency.
#[derive(Debug, Clone)]
pub struct NewBudgetLimit {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub amount: String,
    pub currency: Option<String>,
    pub notes: Option<String>,
}

/// `None` keeps a value; `notes: Some(None)` clears the notes.
#[derive(Debug, Clone, Default)]
pub struct BudgetLimitUpdate {
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub amount: Option<String>,
    pub currency: Option<String>,
    pub notes: Option<Option<String>>,
}

const OVERLAP: &str = "another limit of this budget in this currency overlaps these dates";

impl BudgetService {
    pub async fn create_limit(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        budget_id: BudgetId,
        input: NewBudgetLimit,
    ) -> Result<BudgetLimitView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let result = self
            .create_limit_in(&mut tx, ctx, origin, budget_id, input)
            .await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Like [`Self::create_limit`], inside the caller's transaction.
    pub async fn create_limit_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        budget_id: BudgetId,
        input: NewBudgetLimit,
    ) -> Result<BudgetLimitView, AppError> {
        let household_id = ctx.household_id.to_string();
        let budget = repo::find(conn, &household_id, &budget_id.to_string()).await?;
        let code = match input.currency {
            Some(code) => code,
            None => identity::primary_currency(conn, ctx).await?,
        };
        let currency = currencies::find(conn, &code, "currency").await?;
        let now = format_timestamp(self.clock.now());
        let row = LimitRow {
            id: new_id(&*self.clock).to_string(),
            budget_id: budget.id,
            start_date: input.start_date.to_string(),
            end_date: input.end_date.to_string(),
            amount_minor: positive_amount("amount", &input.amount, &currency)?.minor(),
            currency_code: code,
            notes: clean_notes(input.notes),
            created_at: now.clone(),
            updated_at: now,
        };
        self.write_limit(conn, ctx, origin, row, "created").await
    }

    pub async fn update_limit(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
        update: BudgetLimitUpdate,
    ) -> Result<BudgetLimitView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let result = self
            .update_limit_in(&mut tx, ctx, origin, id, update)
            .await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Like [`Self::update_limit`], inside the caller's transaction.
    pub async fn update_limit_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
        update: BudgetLimitUpdate,
    ) -> Result<BudgetLimitView, AppError> {
        let current = repo::find_limit(conn, &ctx.household_id.to_string(), id).await?;
        let code = update.currency.unwrap_or(current.currency_code.clone());
        let currency = currencies::find(conn, &code, "currency").await?;
        let amount_minor = match &update.amount {
            Some(text) => positive_amount("amount", text, &currency)?.minor(),
            None if code == current.currency_code => current.amount_minor,
            None => {
                return Err(AppError::invalid(
                    "amount",
                    "is required with a new currency",
                ));
            }
        };
        let row = LimitRow {
            start_date: update
                .start_date
                .map_or(current.start_date.clone(), |d| d.to_string()),
            end_date: update
                .end_date
                .map_or(current.end_date.clone(), |d| d.to_string()),
            amount_minor,
            currency_code: code,
            notes: update.notes.map_or(current.notes.clone(), clean_notes),
            updated_at: format_timestamp(self.clock.now()),
            ..current
        };
        self.write_limit(conn, ctx, origin, row, "updated").await
    }

    /// Checks the dates, inserts or updates the row and appends the event.
    async fn write_limit(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        row: LimitRow,
        verb: &str,
    ) -> Result<BudgetLimitView, AppError> {
        if row.end_date < row.start_date {
            return Err(AppError::invalid(
                "end_date",
                "must not be before start_date",
            ));
        }
        let household_id = ctx.household_id.to_string();
        if repo::overlapping_limit(conn, &household_id, &row).await? {
            return Err(AppError::Conflict(OVERLAP.to_string()));
        }
        if verb == "created" {
            repo::insert_limit(conn, &household_id, &row).await?;
        } else {
            repo::update_limit(conn, &household_id, &row).await?;
        }
        let view = self.limit_view(conn, ctx, row).await?;
        let event = NewEvent::new(&*self.clock, "budget_limit", verb, view.id.clone(), &view)?;
        append_event(&mut *conn, ctx, origin, event).await?;
        Ok(view)
    }

    pub async fn delete_limit(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
    ) -> Result<(), AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        self.delete_limit_in(&mut tx, ctx, origin, id).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Like [`Self::delete_limit`], inside the caller's transaction.
    pub async fn delete_limit_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
    ) -> Result<(), AppError> {
        let household_id = ctx.household_id.to_string();
        let row = repo::find_limit(conn, &household_id, id).await?;
        let view = self.limit_view(conn, ctx, row).await?;
        repo::delete_limit(conn, &household_id, id).await?;
        let event = NewEvent::new(
            &*self.clock,
            "budget_limit",
            "deleted",
            view.id.clone(),
            &view,
        )?;
        append_event(&mut *conn, ctx, origin, event).await?;
        Ok(())
    }

    pub async fn get_limit(
        &self,
        ctx: &HouseholdCtx,
        id: &str,
    ) -> Result<BudgetLimitView, AppError> {
        let mut conn = self.pool.acquire().await?;
        self.get_limit_in(&mut conn, ctx, id).await
    }

    /// Like [`Self::get_limit`], inside the caller's transaction.
    pub async fn get_limit_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        id: &str,
    ) -> Result<BudgetLimitView, AppError> {
        let row = repo::find_limit(conn, &ctx.household_id.to_string(), id).await?;
        self.limit_view(conn, ctx, row).await
    }

    /// The budget's limits overlapping `from..=to` (each optional), newest first.
    pub async fn list_limits(
        &self,
        ctx: &HouseholdCtx,
        budget_id: BudgetId,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
        page: PageRequest,
    ) -> Result<Page<BudgetLimitView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        self.list_limits_in(&mut conn, ctx, budget_id, from, to, page)
            .await
    }

    /// Like [`Self::list_limits`], on the caller's connection.
    pub async fn list_limits_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        budget_id: BudgetId,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
        page: PageRequest,
    ) -> Result<Page<BudgetLimitView>, AppError> {
        let household_id = ctx.household_id.to_string();
        let budget = repo::find(conn, &household_id, &budget_id.to_string()).await?;
        let (from, to) = (from.map(|d| d.to_string()), to.map(|d| d.to_string()));
        let mut rows = repo::limits(
            conn,
            &household_id,
            &budget.id,
            from.as_deref(),
            to.as_deref(),
        )
        .await?;
        rows.reverse();
        let total = i64::try_from(rows.len()).unwrap_or(i64::MAX);
        let offset = usize::try_from(page.offset()).unwrap_or(usize::MAX);
        let limit = usize::try_from(page.limit()).unwrap_or(usize::MAX);
        let rows: Vec<LimitRow> = rows.into_iter().skip(offset).take(limit).collect();
        let items = limit_views(conn, ctx, &budget.id, rows).await?;
        Ok(Page::new(items, page, total))
    }

    async fn limit_view(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        row: LimitRow,
    ) -> Result<BudgetLimitView, AppError> {
        let budget_id = row.budget_id.clone();
        let mut views = limit_views(conn, ctx, &budget_id, vec![row]).await?;
        views.pop().ok_or(AppError::NotFound)
    }
}

/// Views of several limits of one budget, loading the spends once. With a
/// carry, a limit depends on every limit before it, so the whole chain up to
/// the latest requested limit is loaded (a few hundred rows at most).
pub(crate) async fn limit_views(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    budget_id: &str,
    rows: Vec<LimitRow>,
) -> Result<Vec<BudgetLimitView>, AppError> {
    let Some(to) = rows.iter().map(|r| r.end_date.clone()).max() else {
        return Ok(Vec::new());
    };
    let household_id = ctx.household_id.to_string();
    let budget = repo::find(conn, &household_id, budget_id).await?;
    let carry = view::core_carry(&budget)?;
    let chain = match carry {
        Carry::None => rows.clone(),
        _ => repo::limits(conn, &household_id, budget_id, None, Some(&to)).await?,
    };
    let from = chain
        .iter()
        .chain(&rows)
        .map(|r| r.start_date.clone())
        .min()
        .unwrap_or_else(|| to.clone());
    let spends = repo::spends(conn, &household_id, budget_id, &from, &to).await?;
    let spends = view::core_spends(&spends)?;
    let carried = carried_by_id(&chain, &spends, carry)?;
    let mut views = Vec::with_capacity(rows.len());
    for row in rows {
        let currency = parse_stored(&row.currency_code)?;
        let carried_in = carried
            .get(&row.id)
            .copied()
            .unwrap_or(Money::zero(currency));
        views.push(view::limit_view(conn, row, &spends, carried_in).await?);
    }
    Ok(views)
}

/// What each limit of `chain` (sorted by start date) carries in, by limit ID.
fn carried_by_id(
    chain: &[LimitRow],
    spends: &[Spend],
    carry: Carry,
) -> Result<HashMap<String, Money>, AppError> {
    let limits = chain
        .iter()
        .map(view::core_limit)
        .collect::<Result<Vec<_>, _>>()?;
    let carried = saldo_core::carried(&limits, spends, carry)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(chain.iter().map(|r| r.id.clone()).zip(carried).collect())
}
