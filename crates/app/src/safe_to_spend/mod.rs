//! "Safe to spend this month" (Phase 10): this month's income, received and
//! planned, minus what was spent and what is committed: bills, planned
//! withdrawals, what budgets have left, piggy bank targets and transfers
//! into savings. The sum is `saldo_core::safe_to_spend`; this module
//! gathers the parts, in the primary currency only.

mod parts;
mod view;

use chrono::{Datelike, Days, Months, NaiveDate};
use saldo_core::safe_to_spend::{Parts, per_day, safe_to_spend};
use saldo_core::{Currency, Money};
use sqlx::SqlitePool;

use crate::budgets::BudgetService;
use crate::clock::SharedClock;
use crate::ctx::HouseholdCtx;
use crate::currencies;
use crate::error::AppError;
use crate::identity::{self, today_in};
use crate::reports::ReportService;
use crate::scope::Scope;

pub use view::{SafeToSpendItem, SafeToSpendParts, SafeToSpendView};

/// One amount of a part, before formatting.
#[derive(Debug, Clone)]
pub(crate) struct Item {
    pub part: &'static str,
    pub source: String,
    pub source_id: String,
    pub title: String,
    pub date: Option<String>,
    pub amount: i64,
}

#[derive(Clone)]
pub struct SafeToSpendService {
    pool: SqlitePool,
    clock: SharedClock,
    budgets: BudgetService,
    reports: ReportService,
}

/// The current month around `today`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Month {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub today: NaiveDate,
}

impl SafeToSpendService {
    pub fn new(
        pool: SqlitePool,
        clock: SharedClock,
        budgets: BudgetService,
        reports: ReportService,
    ) -> Self {
        Self {
            pool,
            clock,
            budgets,
            reports,
        }
    }

    pub async fn current(
        &self,
        ctx: &HouseholdCtx,
        scope: Scope,
    ) -> Result<SafeToSpendView, AppError> {
        let (today, currency) = {
            let mut conn = self.pool.acquire().await?;
            let today = today_in(&mut conn, ctx, &*self.clock).await?;
            let code = identity::primary_currency(&mut conn, ctx).await?;
            (today, currencies::find(&mut conn, &code, "currency").await?)
        };
        let month = month_of(today)?;
        let code = currency.code().to_string();
        let (income_received, spent) =
            parts::cash_flow(&self.reports, ctx, scope, month, &currency).await?;
        let mut items =
            parts::planned(&self.pool, &self.reports, ctx, scope, month, &currency).await?;
        items.extend(parts::budgets(&self.budgets, ctx, month, &currency).await?);
        items.extend(parts::piggy_banks(&self.pool, ctx, month, &code).await?);
        let total = |part: &str| {
            items
                .iter()
                .filter(|i| i.part == part)
                .map(|i| i.amount)
                .sum()
        };
        let sums = Parts {
            income_received,
            income_planned: total("income_planned"),
            spent,
            bills: total("bills"),
            planned: total("planned"),
            budgets: total("budgets"),
            piggy_banks: total("piggy_banks"),
            savings_transfers: total("savings_transfers"),
        };
        let result = safe_to_spend(&sums)
            .ok_or_else(|| AppError::Internal("safe to spend too large".to_string()))?;
        let days_left = (month.end - today).num_days() + 1;
        let money = |minor: i64| Money::new(minor, currency.code()).to_decimal_string(&currency);
        Ok(SafeToSpendView {
            from: month.start.to_string(),
            to: month.end.to_string(),
            currency: code,
            safe_to_spend: money(result),
            per_day: money(per_day(result, days_left)),
            days_left,
            parts: parts_view(&sums, &currency),
            items: items
                .into_iter()
                .map(|i| SafeToSpendItem {
                    part: i.part.to_string(),
                    source: i.source,
                    source_id: i.source_id,
                    title: i.title,
                    date: i.date,
                    amount: money(i.amount),
                })
                .collect(),
        })
    }
}

fn month_of(today: NaiveDate) -> Result<Month, AppError> {
    let start = today
        .with_day(1)
        .ok_or_else(|| AppError::Internal("no first day".to_string()))?;
    let end = start
        .checked_add_months(Months::new(1))
        .and_then(|next| next.checked_sub_days(Days::new(1)))
        .ok_or_else(|| AppError::Internal("no last day".to_string()))?;
    Ok(Month { start, end, today })
}

fn parts_view(p: &Parts, currency: &Currency) -> SafeToSpendParts {
    let money = |minor: i64| Money::new(minor, currency.code()).to_decimal_string(currency);
    SafeToSpendParts {
        income_received: money(p.income_received),
        income_planned: money(p.income_planned),
        spent: money(p.spent),
        bills: money(p.bills),
        planned: money(p.planned),
        budgets: money(p.budgets),
        piggy_banks: money(p.piggy_banks),
        savings_transfers: money(p.savings_transfers),
    }
}
