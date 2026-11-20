//! Year over year: one metric per month for the last few years.

use chrono::{Datelike, NaiveDate};
use saldo_core::ratios::change_bp;

use super::flows::{category_rows, flow_rows};
use super::series::{DayRow, fold};
use super::view::{YearOverYearCurrency, YearOverYearReport, YearOverYearYear};
use super::{Formatter, Interval, ReportQuery, ReportService, minus};
use crate::ctx::HouseholdCtx;
use crate::error::AppError;
use crate::scope::Scope;

pub const MIN_YEARS: u32 = 2;
pub const MAX_YEARS: u32 = 5;
const MONTHS: usize = 12;

/// What each month's amount is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Metric {
    #[default]
    Expenses,
    Income,
    /// Income minus expenses.
    Savings,
}

impl Metric {
    pub fn parse(text: &str) -> Result<Self, AppError> {
        match text {
            "expenses" => Ok(Metric::Expenses),
            "income" => Ok(Metric::Income),
            "savings" => Ok(Metric::Savings),
            _ => Err(AppError::invalid(
                "metric",
                "must be expenses, income or savings",
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Metric::Expenses => "expenses",
            Metric::Income => "income",
            Metric::Savings => "savings",
        }
    }

    fn value(self, (income, expenses): (i64, i64)) -> Result<i64, AppError> {
        match self {
            Metric::Expenses => Ok(expenses),
            Metric::Income => Ok(income),
            Metric::Savings => minus(income, expenses),
        }
    }
}

/// `None`: every split; `Some(None)`: splits without a category.
pub type CategoryFilter = Option<Option<String>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YearOverYearQuery {
    pub years: u32,
    pub metric: Metric,
    pub category: CategoryFilter,
    pub scope: Scope,
}

impl ReportService {
    /// January to December of the last `years` years, ending with `today`'s.
    pub async fn year_over_year(
        &self,
        ctx: &HouseholdCtx,
        today: NaiveDate,
        query: &YearOverYearQuery,
    ) -> Result<YearOverYearReport, AppError> {
        if !(MIN_YEARS..=MAX_YEARS).contains(&query.years) {
            return Err(AppError::invalid(
                "years",
                format!("must be between {MIN_YEARS} and {MAX_YEARS}"),
            ));
        }
        let last = today.year();
        let first = last - i32::try_from(query.years - 1).unwrap_or(0);
        let span = ReportQuery {
            from: jan_first(first)?,
            to: dec_last(last)?,
            interval: Interval::Month,
            scope: query.scope,
        };
        let intervals = span.intervals()?;
        let mut conn = self.pool.acquire().await?;
        let rows = match &query.category {
            None => flow_rows(&mut conn, ctx, &span).await?,
            Some(wanted) => only_category(category_rows(&mut conn, ctx, &span).await?, wanted),
        };
        let grouped = fold(&intervals, rows)?;
        let fmt = Formatter::load(&mut conn, ctx, grouped.keys()).await?;
        let shown = usize::try_from(today.month()).unwrap_or(MONTHS);
        let mut currencies = Vec::new();
        for code in fmt.order(grouped.keys().cloned().collect()) {
            let Some(series) = grouped.get(&code).and_then(|g| g.get(&None)) else {
                continue;
            };
            let values = series
                .intervals
                .iter()
                .map(|&pair| query.metric.value(pair))
                .collect::<Result<Vec<_>, _>>()?;
            let years = years_of(&values, first, shown, &|minor| fmt.format(&code, minor))?;
            currencies.push(YearOverYearCurrency {
                currency: code.clone(),
                years,
            });
        }
        Ok(YearOverYearReport {
            metric: query.metric.as_str().to_string(),
            category_id: query
                .category
                .clone()
                .map(|c| c.unwrap_or_else(|| "none".to_string())),
            currencies,
        })
    }
}

/// Keeps one category's rows (or those without one) under the `None` key,
/// so they fold into one series.
fn only_category(rows: Vec<DayRow>, wanted: &Option<String>) -> Vec<DayRow> {
    rows.into_iter()
        .filter(|row| &row.key == wanted)
        .map(|row| DayRow { key: None, ..row })
        .collect()
}

/// Cuts the monthly values (oldest January first) into years. The last
/// year shows its first `shown` months only.
fn years_of(
    values: &[i64],
    first: i32,
    shown: usize,
    money: &dyn Fn(i64) -> Result<String, AppError>,
) -> Result<Vec<YearOverYearYear>, AppError> {
    let chunks: Vec<&[i64]> = values.chunks(MONTHS).collect();
    let mut years = Vec::with_capacity(chunks.len());
    for (index, months) in chunks.iter().enumerate() {
        let is_last = index + 1 == chunks.len();
        let visible = if is_last { shown } else { MONTHS };
        let previous = index.checked_sub(1).map(|i| chunks[i]);
        let total = sum(&months[..visible])?;
        let before = previous.map(|p| sum(&p[..visible])).transpose()?;
        let mut month_change_bp = vec![None; MONTHS];
        let mut shown_months = vec![None; MONTHS];
        for m in 0..visible {
            shown_months[m] = Some(money(months[m])?);
            month_change_bp[m] = previous.and_then(|p| change_bp(months[m], p[m]));
        }
        years.push(YearOverYearYear {
            year: first + i32::try_from(index).unwrap_or(0),
            months: shown_months,
            month_change_bp,
            total: money(total)?,
            change: before
                .map(|b| minus(total, b).and_then(money))
                .transpose()?,
            change_bp: before.and_then(|b| change_bp(total, b)),
        });
    }
    Ok(years)
}

fn sum(values: &[i64]) -> Result<i64, AppError> {
    values.iter().try_fold(0i64, |acc, &v| {
        acc.checked_add(v)
            .ok_or_else(|| AppError::Internal("report total too large".to_string()))
    })
}

fn jan_first(year: i32) -> Result<NaiveDate, AppError> {
    NaiveDate::from_ymd_opt(year, 1, 1).ok_or_else(|| AppError::invalid("years", "out of range"))
}

fn dec_last(year: i32) -> Result<NaiveDate, AppError> {
    NaiveDate::from_ymd_opt(year, 12, 31).ok_or_else(|| AppError::invalid("years", "out of range"))
}
