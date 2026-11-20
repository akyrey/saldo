//! Gathering the parts of "safe to spend", each as amounts in minor units of
//! the primary currency.

use std::collections::{BTreeMap, HashSet};

use chrono::NaiveDate;
use saldo_core::safe_to_spend::{piggy_bank_need, prorate};
use saldo_core::{Currency, Money};
use sqlx::SqlitePool;

use super::{Item, Month};
use crate::budgets::{BudgetFilter, BudgetService};
use crate::ctx::HouseholdCtx;
use crate::error::{AppError, parse_stored};
use crate::page::PageRequest;
use crate::reports::{ForecastQuery, Interval, ReportQuery, ReportService};
use crate::scope::Scope;

fn minor(text: &str, currency: &Currency) -> Result<i64, AppError> {
    Money::parse_decimal(text, currency)
        .map(|m| m.minor())
        .map_err(|e| AppError::Internal(format!("bad amount {text:?}: {e}")))
}

/// Income received and money spent from the 1st of the month to today.
pub(super) async fn cash_flow(
    reports: &ReportService,
    ctx: &HouseholdCtx,
    scope: Scope,
    month: Month,
    currency: &Currency,
) -> Result<(i64, i64), AppError> {
    let query = ReportQuery {
        from: month.start,
        to: month.today,
        interval: Interval::Month,
        scope,
    };
    let report = reports.cash_flow(ctx, &query).await?;
    let code = currency.code().to_string();
    match report.currencies.iter().find(|c| c.currency == code) {
        Some(c) => Ok((minor(&c.income, currency)?, minor(&c.expenses, currency)?)),
        None => Ok((0, 0)),
    }
}

/// The forecast's items for the rest of the month, one per planned
/// transaction, recurrence date or bill. Amounts are per account side, so a
/// planned transfer between two counted accounts sums to zero: it is a
/// savings transfer when money lands in a savings account, otherwise
/// nothing. A positive sum is planned income; a negative one a bill or a
/// planned withdrawal.
pub(super) async fn planned(
    pool: &SqlitePool,
    reports: &ReportService,
    ctx: &HouseholdCtx,
    scope: Scope,
    month: Month,
    currency: &Currency,
) -> Result<Vec<Item>, AppError> {
    if month.end <= month.today {
        return Ok(Vec::new());
    }
    let query = ForecastQuery {
        to: month.end,
        interval: Interval::Week,
        scope,
        account_id: None,
    };
    let report = reports.forecast(ctx, month.today, &query).await?;
    let code = currency.code().to_string();
    let Some(projected) = report.currencies.into_iter().find(|c| c.currency == code) else {
        return Ok(Vec::new());
    };
    let savings = savings_accounts(pool, ctx).await?;
    // (date, source, source id) → (title, sum, moved into savings).
    let mut groups: BTreeMap<(String, String, String), (String, i64, i64)> = BTreeMap::new();
    for item in projected.items {
        let amount = minor(&item.amount, currency)?;
        let into_savings = match &item.account_id {
            Some(account) if amount > 0 && savings.contains(account) => amount,
            _ => 0,
        };
        let entry = groups
            .entry((item.date, item.source, item.source_id))
            .or_insert((item.title, 0, 0));
        entry.1 += amount;
        entry.2 += into_savings;
    }
    let mut items = Vec::new();
    for ((date, source, source_id), (title, sum, into_savings)) in groups {
        let (part, amount) = match sum {
            s if s > 0 => ("income_planned", s),
            s if s < 0 && source == "bill" => ("bills", -s),
            s if s < 0 => ("planned", -s),
            _ if into_savings > 0 => ("savings_transfers", into_savings),
            _ => continue,
        };
        items.push(Item {
            part,
            source,
            source_id,
            title,
            date: Some(date),
            amount,
        });
    }
    Ok(items)
}

async fn savings_accounts(
    pool: &SqlitePool,
    ctx: &HouseholdCtx,
) -> Result<HashSet<String>, AppError> {
    let household_id = ctx.household_id.to_string();
    let ids = sqlx::query_scalar!(
        "SELECT id FROM accounts WHERE household_id = ? AND type = 'asset' AND asset_role = 'savings'",
        household_id
    )
    .fetch_all(pool)
    .await?;
    Ok(ids.into_iter().collect())
}

/// What each active budget has left for the rest of the month, in the
/// primary currency: limits overlapping today to the month end, prorated by
/// days when they run past it.
pub(super) async fn budgets(
    budgets: &BudgetService,
    ctx: &HouseholdCtx,
    month: Month,
    currency: &Currency,
) -> Result<Vec<Item>, AppError> {
    let code = currency.code().to_string();
    let filter = BudgetFilter { active: Some(true) };
    let mut items = Vec::new();
    for budget in budgets.list(ctx, &filter, PageRequest::max()).await?.items {
        let id = parse_stored(&budget.id)?;
        let limits = budgets
            .list_limits(
                ctx,
                id,
                Some(month.today),
                Some(month.end),
                PageRequest::max(),
            )
            .await?;
        let mut reserve = 0;
        for limit in limits.items.iter().filter(|l| l.currency == code) {
            let start: NaiveDate = parse_stored(&limit.start_date)?;
            let end: NaiveDate = parse_stored(&limit.end_date)?;
            let available = minor(&limit.available, currency)?;
            reserve += prorate(available, start.max(month.today), month.end, end);
        }
        if reserve > 0 {
            items.push(Item {
                part: "budgets",
                source: "budget".to_string(),
                source_id: budget.id,
                title: budget.name,
                date: None,
                amount: reserve,
            });
        }
    }
    Ok(items)
}

/// What each active piggy bank with a target amount and a target date (in
/// the primary currency) needs this month. Piggy banks belong to the whole
/// household, so the scope does not apply.
pub(super) async fn piggy_banks(
    pool: &SqlitePool,
    ctx: &HouseholdCtx,
    month: Month,
    code: &str,
) -> Result<Vec<Item>, AppError> {
    let household_id = ctx.household_id.to_string();
    let (start, today) = (month.start.to_string(), month.today.to_string());
    let rows = sqlx::query!(
        r#"SELECT p.id AS "id!", p.name AS "name!", p.target_amount_minor AS "target!: i64",
                  p.target_date AS "target_date!",
                  COALESCE(SUM(e.amount_minor), 0) AS "saved!: i64",
                  COALESCE(SUM(CASE WHEN e.date >= ?2 THEN e.amount_minor ELSE 0 END), 0) AS "added!: i64"
           FROM piggy_banks p
           LEFT JOIN piggy_bank_events e ON e.piggy_bank_id = p.id AND e.household_id = p.household_id
           WHERE p.household_id = ?1 AND p.active = 1 AND p.currency_code = ?4
             AND p.target_amount_minor IS NOT NULL AND p.target_date IS NOT NULL
             AND p.target_date >= ?3
           GROUP BY p.id ORDER BY p.sort_order, p.name"#,
        household_id,
        start,
        today,
        code,
    )
    .fetch_all(pool)
    .await?;
    let mut items = Vec::new();
    for row in rows {
        let due: NaiveDate = parse_stored(&row.target_date)?;
        let need = piggy_bank_need(row.target, row.saved, row.added, month.today, due);
        if need > 0 {
            items.push(Item {
                part: "piggy_banks",
                source: "piggy_bank".to_string(),
                source_id: row.id,
                title: row.name,
                date: Some(row.target_date),
                amount: need,
            });
        }
    }
    Ok(items)
}
