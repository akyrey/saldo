//! The weekly digest: last week's income and spending, the top categories,
//! the bills due in the coming week, and the budgets past 80%.

use chrono::{Days, NaiveDate};
use saldo_core::Money;
use saldo_core::alerts::DigestWeek;
use serde_json::{Value, json};
use sqlx::SqlitePool;

use super::Condition;
use super::budgets::BudgetOver;
use crate::ctx::HouseholdCtx;
use crate::currencies;
use crate::error::AppError;
use crate::reports::{Interval, ReportQuery, ReportService};
use crate::scope::Scope;

/// How many categories the digest names.
const TOP_CATEGORIES: usize = 3;
/// How far ahead "bills due" looks, from today.
const BILLS_AHEAD_DAYS: u64 = 6;

pub(super) async fn condition(
    pool: &SqlitePool,
    reports: &ReportService,
    ctx: &HouseholdCtx,
    key: String,
    week: DigestWeek,
    today: NaiveDate,
    over: &[BudgetOver],
) -> Result<Condition, AppError> {
    let query = ReportQuery {
        from: week.from,
        to: week.to,
        interval: Interval::Week,
        scope: Scope::Household,
    };
    let flow = reports.cash_flow(ctx, &query).await?;
    let currencies: Vec<Value> = flow
        .currencies
        .iter()
        .map(|c| json!({ "currency": c.currency, "income": c.income, "expenses": c.expenses }))
        .collect();
    let categories = reports.by_category(ctx, &query).await?;
    let top: Vec<Value> = categories
        .currencies
        .iter()
        .flat_map(|c| {
            c.items
                .iter()
                .filter(|item| !is_zero(&item.expenses))
                .take(TOP_CATEGORIES)
                .map(|item| json!({ "name": item.name, "expenses": item.expenses, "currency": c.currency }))
        })
        .collect();
    let budgets: Vec<Value> = over
        .iter()
        .map(|b| json!({ "name": b.name, "percent": b.percent }))
        .collect();
    let data = json!({
        "week": { "from": week.from.to_string(), "to": week.to.to_string() },
        "currencies": currencies,
        "top_categories": top,
        "bills_due": bills_due(pool, ctx, today).await?,
        "budgets": budgets,
    });
    Ok(Condition {
        entity_id: key.trim_start_matches("digest:").to_string(),
        key,
        entity_type: "digest",
        action: "weekly",
        data,
    })
}

/// `"0.00"`, `"0"`: nothing spent in a category this week.
fn is_zero(amount: &str) -> bool {
    amount.chars().all(|c| matches!(c, '0' | '.' | '-'))
}

/// Open bill periods due from today to six days ahead, at the bill's
/// highest amount.
async fn bills_due(
    pool: &SqlitePool,
    ctx: &HouseholdCtx,
    today: NaiveDate,
) -> Result<Vec<Value>, AppError> {
    let household_id = ctx.household_id.to_string();
    let (from, to) = (
        today.to_string(),
        (today + Days::new(BILLS_AHEAD_DAYS)).to_string(),
    );
    let rows = sqlx::query!(
        "SELECT b.name, p.due_date, b.amount_max_minor, b.currency_code
         FROM bill_periods p JOIN bills b ON b.id = p.bill_id AND b.household_id = p.household_id
         WHERE p.household_id = ? AND p.status = 'open' AND b.active = 1
           AND p.due_date >= ? AND p.due_date <= ?
         ORDER BY p.due_date, b.name",
        household_id,
        from,
        to
    )
    .fetch_all(pool)
    .await?;
    let mut conn = pool.acquire().await?;
    let mut bills = Vec::with_capacity(rows.len());
    for row in rows {
        let currency = currencies::find(&mut conn, &row.currency_code, "currency").await?;
        let amount = Money::new(row.amount_max_minor, currency.code()).to_decimal_string(&currency);
        bills.push(json!({
            "name": row.name,
            "due_date": row.due_date,
            "amount": amount,
            "currency": row.currency_code,
        }));
    }
    Ok(bills)
}
