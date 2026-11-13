//! Cash flow, and income and expenses by category and by tag.

use chrono::NaiveDate;
use saldo_core::DateRange;
use sqlx::SqliteConnection;

use super::series::{DayRow, Grouped, fold};
use super::view::*;
use super::{Formatter, ReportQuery, ReportService, minus, parse_day};
use crate::ctx::HouseholdCtx;
use crate::error::AppError;

/// What a breakdown report groups by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum By {
    Category,
    Tag,
}

impl ReportService {
    /// Income and expenses per interval and currency.
    pub async fn cash_flow(
        &self,
        ctx: &HouseholdCtx,
        query: &ReportQuery,
    ) -> Result<CashFlowReport, AppError> {
        let intervals = query.intervals()?;
        let mut conn = self.pool.acquire().await?;
        let rows = flow_rows(&mut conn, ctx, query).await?;
        let grouped = fold(&intervals, rows)?;
        let fmt = Formatter::load(&mut conn, ctx, grouped.keys()).await?;
        let mut currencies = Vec::new();
        for code in fmt.order(grouped.keys().cloned().collect()) {
            let Some(series) = grouped.get(&code).and_then(|g| g.get(&None)) else {
                continue;
            };
            let money = |minor| fmt.format(&code, minor);
            let mut rows = Vec::with_capacity(intervals.len());
            for (range, &(income, expenses)) in intervals.iter().zip(&series.intervals) {
                rows.push(CashFlowInterval {
                    start: range.start.to_string(),
                    end: range.end.to_string(),
                    income: money(income)?,
                    expenses: money(expenses)?,
                    net: money(minus(income, expenses)?)?,
                });
            }
            let (income, expenses) = series.total;
            currencies.push(CashFlowCurrency {
                income: money(income)?,
                expenses: money(expenses)?,
                net: money(minus(income, expenses)?)?,
                intervals: rows,
                currency: code.clone(),
            });
        }
        Ok(CashFlowReport {
            range: query.range(),
            currencies,
        })
    }

    /// Income and expenses per category (`id: null` without one).
    pub async fn by_category(
        &self,
        ctx: &HouseholdCtx,
        query: &ReportQuery,
    ) -> Result<BreakdownReport, AppError> {
        self.breakdown(ctx, query, By::Category).await
    }

    /// Income and expenses per tag; a split counts for each of its tags.
    pub async fn by_tag(
        &self,
        ctx: &HouseholdCtx,
        query: &ReportQuery,
    ) -> Result<BreakdownReport, AppError> {
        self.breakdown(ctx, query, By::Tag).await
    }

    async fn breakdown(
        &self,
        ctx: &HouseholdCtx,
        query: &ReportQuery,
        by: By,
    ) -> Result<BreakdownReport, AppError> {
        let intervals = query.intervals()?;
        let mut conn = self.pool.acquire().await?;
        let rows = match by {
            By::Category => category_rows(&mut conn, ctx, query).await?,
            By::Tag => tag_rows(&mut conn, ctx, query).await?,
        };
        let grouped = fold(&intervals, rows)?;
        let fmt = Formatter::load(&mut conn, ctx, grouped.keys()).await?;
        Ok(BreakdownReport {
            range: query.range(),
            currencies: breakdown_currencies(&fmt, &intervals, grouped)?,
        })
    }
}

fn breakdown_currencies(
    fmt: &Formatter,
    intervals: &[DateRange],
    mut grouped: Grouped,
) -> Result<Vec<BreakdownCurrency>, AppError> {
    let mut currencies = Vec::new();
    for code in fmt.order(grouped.keys().cloned().collect()) {
        let Some(groups) = grouped.remove(&code) else {
            continue;
        };
        let mut groups: Vec<_> = groups.into_iter().collect();
        // Largest expenses first, then by name: a closure as the sort key,
        // like `usort` with a callback.
        groups.sort_by(|(_, a), (_, b)| b.total.1.cmp(&a.total.1).then(a.name.cmp(&b.name)));
        let money = |minor| fmt.format(&code, minor);
        let mut items = Vec::with_capacity(groups.len());
        for (id, series) in groups {
            let mut rows = Vec::with_capacity(intervals.len());
            for (range, &(income, expenses)) in intervals.iter().zip(&series.intervals) {
                rows.push(BreakdownInterval {
                    start: range.start.to_string(),
                    end: range.end.to_string(),
                    income: money(income)?,
                    expenses: money(expenses)?,
                });
            }
            items.push(BreakdownItem {
                id,
                name: series.name,
                income: money(series.total.0)?,
                expenses: money(series.total.1)?,
                intervals: rows,
            });
        }
        currencies.push(BreakdownCurrency {
            currency: code.clone(),
            items,
        });
    }
    Ok(currencies)
}

/// Turns one SQL row into a [`DayRow`].
fn day_row(
    date: &str,
    currency: String,
    key: Option<String>,
    name: Option<String>,
    income: i64,
    expenses: i64,
) -> Result<DayRow, AppError> {
    let date: NaiveDate = parse_day(date)?;
    Ok(DayRow {
        date,
        currency,
        key,
        name,
        first: income,
        second: expenses,
    })
}

// In the queries below, `?4` is the account owner of the scope (NULL for the
// whole household): a split counts if either of its accounts is the owner's,
// the same rule as the transaction list.

async fn flow_rows(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    query: &ReportQuery,
) -> Result<Vec<DayRow>, AppError> {
    let (household_id, owner) = (ctx.household_id.to_string(), query.scope.owner(ctx));
    let (from, to) = (query.from.to_string(), query.to.to_string());
    let rows = sqlx::query!(
        r#"SELECT s.date AS "date!", s.currency_code AS "currency!",
                  SUM(CASE WHEN s.kind = 'deposit' THEN s.amount_minor ELSE 0 END) AS "income!: i64",
                  SUM(CASE WHEN s.kind = 'withdrawal' THEN s.amount_minor ELSE 0 END) AS "expenses!: i64"
           FROM splits s
           WHERE s.household_id = ?1 AND s.date >= ?2 AND s.date <= ?3
             AND s.kind IN ('withdrawal', 'deposit')
             AND (?4 IS NULL OR EXISTS (SELECT 1 FROM accounts a WHERE a.household_id = s.household_id
                   AND a.id IN (s.source_account_id, s.destination_account_id) AND a.owner_user_id = ?4))
           GROUP BY s.date, s.currency_code"#,
        household_id,
        from,
        to,
        owner,
    )
    .fetch_all(&mut *conn)
    .await?;
    rows.into_iter()
        .map(|r| day_row(&r.date, r.currency, None, None, r.income, r.expenses))
        .collect()
}

async fn category_rows(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    query: &ReportQuery,
) -> Result<Vec<DayRow>, AppError> {
    let (household_id, owner) = (ctx.household_id.to_string(), query.scope.owner(ctx));
    let (from, to) = (query.from.to_string(), query.to.to_string());
    let rows = sqlx::query!(
        r#"SELECT s.date AS "date!", s.currency_code AS "currency!", c.id AS "key?", c.name AS "name?",
                  SUM(CASE WHEN s.kind = 'deposit' THEN s.amount_minor ELSE 0 END) AS "income!: i64",
                  SUM(CASE WHEN s.kind = 'withdrawal' THEN s.amount_minor ELSE 0 END) AS "expenses!: i64"
           FROM splits s LEFT JOIN categories c ON c.id = s.category_id AND c.household_id = s.household_id
           WHERE s.household_id = ?1 AND s.date >= ?2 AND s.date <= ?3
             AND s.kind IN ('withdrawal', 'deposit')
             AND (?4 IS NULL OR EXISTS (SELECT 1 FROM accounts a WHERE a.household_id = s.household_id
                   AND a.id IN (s.source_account_id, s.destination_account_id) AND a.owner_user_id = ?4))
           GROUP BY s.date, s.currency_code, c.id"#,
        household_id,
        from,
        to,
        owner,
    )
    .fetch_all(&mut *conn)
    .await?;
    rows.into_iter()
        .map(|r| day_row(&r.date, r.currency, r.key, r.name, r.income, r.expenses))
        .collect()
}

async fn tag_rows(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    query: &ReportQuery,
) -> Result<Vec<DayRow>, AppError> {
    let (household_id, owner) = (ctx.household_id.to_string(), query.scope.owner(ctx));
    let (from, to) = (query.from.to_string(), query.to.to_string());
    let rows = sqlx::query!(
        r#"SELECT s.date AS "date!", s.currency_code AS "currency!", t.id AS "key?", t.name AS "name?",
                  SUM(CASE WHEN s.kind = 'deposit' THEN s.amount_minor ELSE 0 END) AS "income!: i64",
                  SUM(CASE WHEN s.kind = 'withdrawal' THEN s.amount_minor ELSE 0 END) AS "expenses!: i64"
           FROM splits s
           LEFT JOIN split_tags st ON st.split_id = s.id
           LEFT JOIN tags t ON t.id = st.tag_id AND t.household_id = s.household_id
           WHERE s.household_id = ?1 AND s.date >= ?2 AND s.date <= ?3
             AND s.kind IN ('withdrawal', 'deposit')
             AND (?4 IS NULL OR EXISTS (SELECT 1 FROM accounts a WHERE a.household_id = s.household_id
                   AND a.id IN (s.source_account_id, s.destination_account_id) AND a.owner_user_id = ?4))
           GROUP BY s.date, s.currency_code, t.id"#,
        household_id,
        from,
        to,
        owner,
    )
    .fetch_all(&mut *conn)
    .await?;
    rows.into_iter()
        .map(|r| day_row(&r.date, r.currency, r.key, r.name, r.income, r.expenses))
        .collect()
}
