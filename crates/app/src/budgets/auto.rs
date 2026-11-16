//! Automatic limits: each budget with an auto-budget gets one limit per
//! period, created up to the current period (household timezone).

use chrono::NaiveDate;
use sqlx::SqliteConnection;

use super::BudgetService;
use super::repo::{self, BudgetRow, LimitRow};
use super::view;
use crate::clock::{format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::AppError;
use crate::events::{NewEvent, append_event};
use crate::identity;

impl BudgetService {
    /// Creates the missing automatic limits of every active budget up to the
    /// period containing `today`, with `Origin::Scheduler`, in one database
    /// transaction. Returns how many limits were created. Safe to call
    /// repeatedly: a covered period is never filled twice.
    pub async fn create_auto_limits(
        &self,
        ctx: &HouseholdCtx,
        today: NaiveDate,
    ) -> Result<usize, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let budgets = repo::all(&mut tx, &ctx.household_id.to_string()).await?;
        let mut created = 0;
        for budget in budgets.iter().filter(|b| b.active) {
            created += self
                .generate(&mut tx, ctx, Origin::Scheduler, budget, today)
                .await?;
        }
        tx.commit().await?;
        Ok(created)
    }

    /// After a budget was written: its automatic limits up to the household's today.
    pub(super) async fn auto_limits_for(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        budget: &BudgetRow,
    ) -> Result<usize, AppError> {
        if !budget.active || budget.auto_type.is_none() {
            return Ok(0);
        }
        let today = identity::today_in(conn, ctx, &*self.clock).await?;
        self.generate(conn, ctx, origin, budget, today).await
    }

    async fn generate(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        budget: &BudgetRow,
        today: NaiveDate,
    ) -> Result<usize, AppError> {
        let Some(auto) = view::core_auto_budget(conn, budget).await? else {
            return Ok(0);
        };
        let household_id = ctx.household_id.to_string();
        let last = repo::last_limit(conn, &household_id, &budget.id).await?;
        let last = last.as_ref().map(view::core_limit).transpose()?;
        // Spends from the last limit (for a rollover) to the current period's end.
        let from = last.map_or(auto.period.start_of(today), |l| l.range.start);
        let to = auto.period.range_of(today).end;
        let rows = repo::spends(
            conn,
            &household_id,
            &budget.id,
            &from.to_string(),
            &to.to_string(),
        )
        .await?;
        let spends = view::core_spends(&rows)?;
        let limits = saldo_core::auto_limits(&auto, last.as_ref(), &spends, today)
            .map_err(|e| AppError::Internal(e.to_string()))?;
        let mut count = 0;
        for limit in limits {
            let now = format_timestamp(self.clock.now());
            let row = LimitRow {
                id: new_id(&*self.clock).to_string(),
                budget_id: budget.id.clone(),
                start_date: limit.range.start.to_string(),
                end_date: limit.range.end.to_string(),
                amount_minor: limit.amount.minor(),
                currency_code: limit.amount.currency().to_string(),
                notes: None,
                created_at: now.clone(),
                updated_at: now,
            };
            // A hand-made limit may already cover part of the period.
            if repo::overlapping_limit(conn, &household_id, &row).await? {
                continue;
            }
            repo::insert_limit(conn, &household_id, &row).await?;
            let view = view::limit_view(conn, row, &spends).await?;
            let event = NewEvent::new(
                &*self.clock,
                "budget_limit",
                "created",
                view.id.clone(),
                &view,
            )?;
            append_event(&mut *conn, ctx, origin, event).await?;
            count += 1;
        }
        Ok(count)
    }
}
