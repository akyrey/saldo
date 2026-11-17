//! Cash-flow forecast: the covered accounts' balance from today up to `to`,
//! from transactions already entered with a later date, recurrences that will
//! fire and bills that will fall due (ARCHITECTURE.md 6, Phase 9).

use std::collections::BTreeMap;

use chrono::{Days, Months, NaiveDate};
use saldo_core::{AccountId, Money, Planned, project};
use serde::Serialize;

use super::forecast_sources::{self as sources, Entry};
use super::{Formatter, Interval, ReportRange, ReportService};
use crate::bills::upcoming::upcoming_in as upcoming_bills;
use crate::ctx::HouseholdCtx;
use crate::error::{AppError, parse_stored};
use crate::recurrences::upcoming::upcoming_in as upcoming_recurrences;
use crate::scope::Scope;

/// How far ahead a forecast may look.
pub const MAX_FORECAST_MONTHS: u32 = 24;

/// What a forecast is asked for. It always starts today (household time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForecastQuery {
    pub to: NaiveDate,
    /// `day`, `week` or `month`.
    pub interval: Interval,
    pub scope: Scope,
    /// One account instead of every account counted in net worth.
    pub account_id: Option<AccountId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct ForecastReport {
    pub range: ReportRange,
    pub currencies: Vec<ForecastCurrency>,
}

/// The projected balance in one currency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct ForecastCurrency {
    pub currency: String,
    /// The balance at the end of today.
    pub start_balance: String,
    /// The balance at the end of each interval.
    pub points: Vec<ForecastPoint>,
    /// The lowest end-of-day balance in the span (the earliest on a tie).
    pub lowest: ForecastPoint,
    /// Every planned amount, by date.
    pub items: Vec<ForecastItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct ForecastPoint {
    pub date: String,
    pub balance: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct ForecastItem {
    pub date: String,
    /// `transaction` (entered with a later date), `recurrence` or `bill`.
    pub source: String,
    /// The transaction group, recurrence or bill.
    pub source_id: String,
    pub title: String,
    /// `None` for a bill nobody has paid yet (counted in the total only).
    pub account_id: Option<String>,
    /// Positive when money comes in, negative when it goes out.
    pub amount: String,
}

impl ReportService {
    /// The forecast from `today` (the household's date) to `query.to`.
    pub async fn forecast(
        &self,
        ctx: &HouseholdCtx,
        today: NaiveDate,
        query: &ForecastQuery,
    ) -> Result<ForecastReport, AppError> {
        let intervals = check(today, query)?;
        let tomorrow = today + Days::new(1);
        let mut conn = self.pool.acquire().await?;
        let account_id = query.account_id.map(|id| id.to_string());
        let accounts =
            sources::accounts(&mut conn, ctx, query.scope, account_id.as_deref()).await?;
        let starts = sources::start_balances(&mut conn, ctx, &accounts, today).await?;
        let mut entries =
            sources::future_splits(&mut conn, ctx, &accounts, today, query.to).await?;
        let recurrences = upcoming_recurrences(&mut conn, ctx, query.to).await?;
        let bills = upcoming_bills(&mut conn, ctx, query.to).await?;
        let whole = query.scope == Scope::Household && query.account_id.is_none();
        entries.extend(sources::recurrence_entries(
            &recurrences,
            &accounts,
            tomorrow,
        ));
        entries.extend(sources::bill_entries(
            &bills,
            &recurrences,
            &accounts,
            tomorrow,
            whole,
        ));
        entries.sort_by(|a, b| a.date.cmp(&b.date).then(a.title.cmp(&b.title)));

        let mut by_currency: BTreeMap<String, Vec<Entry>> = starts
            .keys()
            .map(|code| (code.clone(), Vec::new()))
            .collect();
        for entry in entries {
            by_currency
                .entry(entry.currency.clone())
                .or_default()
                .push(entry);
        }
        let fmt = Formatter::load(&mut conn, ctx, by_currency.keys()).await?;
        let mut currencies = Vec::new();
        for code in fmt.order(by_currency.keys().cloned().collect()) {
            let items = by_currency.remove(&code).unwrap_or_default();
            let start = starts.get(&code).copied().unwrap_or(0);
            currencies.push(currency_view(&fmt, &code, start, items, &intervals)?);
        }
        Ok(ForecastReport {
            range: ReportRange {
                from: today.to_string(),
                to: query.to.to_string(),
                interval: query.interval.as_str().to_string(),
            },
            currencies,
        })
    }
}

/// Checks the span and the interval; returns the intervals from today.
fn check(today: NaiveDate, query: &ForecastQuery) -> Result<Vec<saldo_core::DateRange>, AppError> {
    if query.to <= today {
        return Err(AppError::invalid("to", "must be after today"));
    }
    let limit = today
        .checked_add_months(Months::new(MAX_FORECAST_MONTHS))
        .unwrap_or(NaiveDate::MAX);
    if query.to > limit {
        return Err(AppError::invalid(
            "to",
            format!("must be at most {MAX_FORECAST_MONTHS} months ahead"),
        ));
    }
    if query.interval == Interval::Year {
        return Err(AppError::invalid("interval", "must be day, week or month"));
    }
    Ok(query.interval.period().ranges(today, query.to))
}

fn currency_view(
    fmt: &Formatter,
    code: &str,
    start: i64,
    items: Vec<Entry>,
    intervals: &[saldo_core::DateRange],
) -> Result<ForecastCurrency, AppError> {
    let currency = parse_stored(code)?;
    let planned: Vec<Planned> = items
        .iter()
        .map(|e| Planned {
            date: e.date,
            amount: Money::new(e.amount_minor, currency),
        })
        .collect();
    let projection = project(Money::new(start, currency), &planned, intervals)
        .map_err(|e| AppError::Internal(format!("forecast: {e}")))?;
    let point = |date: NaiveDate, balance: Money| -> Result<ForecastPoint, AppError> {
        Ok(ForecastPoint {
            date: date.to_string(),
            balance: fmt.format(code, balance.minor())?,
        })
    };
    let points = projection
        .points
        .iter()
        .map(|p| point(p.date, p.balance))
        .collect::<Result<Vec<_>, _>>()?;
    let items = items
        .into_iter()
        .map(|e| {
            Ok(ForecastItem {
                date: e.date.to_string(),
                source: e.source.to_string(),
                source_id: e.source_id,
                title: e.title,
                account_id: e.account_id,
                amount: fmt.format(code, e.amount_minor)?,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(ForecastCurrency {
        currency: code.to_string(),
        start_balance: fmt.format(code, start)?,
        points,
        lowest: point(projection.lowest.date, projection.lowest.balance)?,
        items,
    })
}
