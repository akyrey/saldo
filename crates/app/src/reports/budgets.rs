//! Budget vs actual: per budget, the limits that start in each interval and
//! the withdrawals booked on it.

use sqlx::SqliteConnection;

use super::series::{DayRow, fold};
use super::view::*;
use super::{Formatter, ReportQuery, ReportService, parse_day};
use crate::ctx::HouseholdCtx;
use crate::error::AppError;

impl ReportService {
    /// Budgeted and spent per budget, interval and currency. A limit counts
    /// in the interval its start date falls in; spending is filtered by the
    /// scope, limits are not (budgets belong to the household).
    pub async fn budget_vs_actual(
        &self,
        ctx: &HouseholdCtx,
        query: &ReportQuery,
    ) -> Result<BudgetReport, AppError> {
        let intervals = query.intervals()?;
        let mut conn = self.pool.acquire().await?;
        let mut rows = spent_rows(&mut conn, ctx, query).await?;
        rows.extend(budgeted_rows(&mut conn, ctx, query).await?);
        let mut grouped = fold(&intervals, rows)?;
        let fmt = Formatter::load(&mut conn, ctx, grouped.keys()).await?;
        let mut currencies = Vec::new();
        for code in fmt.order(grouped.keys().cloned().collect()) {
            let Some(groups) = grouped.remove(&code) else {
                continue;
            };
            let money = |minor| fmt.format(&code, minor);
            let mut groups: Vec<_> = groups.into_iter().collect();
            // Budgets by name; spending without a budget last.
            groups.sort_by_key(|(id, series)| (id.is_none(), series.name.clone()));
            let mut items = Vec::with_capacity(groups.len());
            for (budget_id, series) in groups {
                let mut rows = Vec::with_capacity(intervals.len());
                for (range, &(budgeted, spent)) in intervals.iter().zip(&series.intervals) {
                    rows.push(BudgetReportInterval {
                        start: range.start.to_string(),
                        end: range.end.to_string(),
                        budgeted: money(budgeted)?,
                        spent: money(spent)?,
                    });
                }
                items.push(BudgetReportItem {
                    budget_id,
                    name: series.name,
                    budgeted: money(series.total.0)?,
                    spent: money(series.total.1)?,
                    intervals: rows,
                });
            }
            currencies.push(BudgetReportCurrency {
                currency: code.clone(),
                items,
            });
        }
        Ok(BudgetReport {
            range: query.range(),
            currencies,
        })
    }
}

async fn spent_rows(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    query: &ReportQuery,
) -> Result<Vec<DayRow>, AppError> {
    let (household_id, owner) = (ctx.household_id.to_string(), query.scope.owner(ctx));
    let (from, to) = (query.from.to_string(), query.to.to_string());
    let rows = sqlx::query!(
        r#"SELECT s.date AS "date!", s.currency_code AS "currency!", b.id AS "key?", b.name AS "name?",
                  SUM(s.amount_minor) AS "spent!: i64"
           FROM splits s LEFT JOIN budgets b ON b.id = s.budget_id AND b.household_id = s.household_id
           WHERE s.household_id = ?1 AND s.date >= ?2 AND s.date <= ?3 AND s.kind = 'withdrawal'
             AND (?4 IS NULL OR EXISTS (SELECT 1 FROM accounts a WHERE a.household_id = s.household_id
                   AND a.id IN (s.source_account_id, s.destination_account_id) AND a.owner_user_id = ?4))
           GROUP BY s.date, s.currency_code, b.id"#,
        household_id,
        from,
        to,
        owner,
    )
    .fetch_all(&mut *conn)
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok(DayRow {
                date: parse_day(&r.date)?,
                currency: r.currency,
                key: r.key,
                name: r.name,
                first: 0,
                second: r.spent,
            })
        })
        .collect()
}

async fn budgeted_rows(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    query: &ReportQuery,
) -> Result<Vec<DayRow>, AppError> {
    let household_id = ctx.household_id.to_string();
    let (from, to) = (query.from.to_string(), query.to.to_string());
    let rows = sqlx::query!(
        r#"SELECT l.start_date AS "date!", l.currency_code AS "currency!", b.id AS "key!", b.name AS "name!",
                  SUM(l.amount_minor) AS "budgeted!: i64"
           FROM budget_limits l JOIN budgets b ON b.id = l.budget_id AND b.household_id = l.household_id
           WHERE l.household_id = ?1 AND l.start_date >= ?2 AND l.start_date <= ?3
           GROUP BY l.start_date, l.currency_code, b.id"#,
        household_id,
        from,
        to,
    )
    .fetch_all(&mut *conn)
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok(DayRow {
                date: parse_day(&r.date)?,
                currency: r.currency,
                key: Some(r.key),
                name: Some(r.name),
                first: r.budgeted,
                second: 0,
            })
        })
        .collect()
}
