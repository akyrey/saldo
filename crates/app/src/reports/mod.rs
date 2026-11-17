//! Reports: net worth over time, cash flow, spending by category, by tag and
//! by budget. SQL sums per day; Rust folds the days into intervals.
//!
//! Every report takes `from`, `to` (inclusive), an interval and a scope.
//! Income is deposits and expenses are withdrawals: transfers between the
//! household's own accounts (and opening balances and reconciliations) are
//! neither. Each currency is reported separately, without conversion.

mod budgets;
mod flows;
mod forecast;
mod forecast_sources;
mod net_worth;
mod series;
mod view;

use std::collections::HashMap;

use chrono::NaiveDate;
use saldo_core::{Currency, DateRange, Money, Period};
use sqlx::{SqliteConnection, SqlitePool};

use crate::ctx::HouseholdCtx;
use crate::error::{AppError, parse_stored};
use crate::scope::Scope;
use crate::{currencies, identity};

pub use forecast::{
    ForecastCurrency, ForecastItem, ForecastPoint, ForecastQuery, ForecastReport,
    MAX_FORECAST_MONTHS,
};
pub use view::*;

/// The most intervals one report may have (about 2.7 years of days).
pub const MAX_INTERVALS: u64 = 1000;

/// How a report cuts its span: `day`, `week` (from Monday), `month`, `year`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interval {
    Day,
    Week,
    #[default]
    Month,
    Year,
}

impl Interval {
    pub fn parse(text: &str) -> Result<Self, AppError> {
        match text {
            "day" => Ok(Interval::Day),
            "week" => Ok(Interval::Week),
            "month" => Ok(Interval::Month),
            "year" => Ok(Interval::Year),
            _ => Err(AppError::invalid(
                "interval",
                "must be day, week, month or year",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Interval::Day => "day",
            Interval::Week => "week",
            Interval::Month => "month",
            Interval::Year => "year",
        }
    }

    fn period(self) -> Period {
        match self {
            Interval::Day => Period::Daily,
            Interval::Week => Period::Weekly,
            Interval::Month => Period::Monthly,
            Interval::Year => Period::Yearly,
        }
    }
}

/// What every report is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportQuery {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub interval: Interval,
    pub scope: Scope,
}

impl ReportQuery {
    /// The intervals of the span, each clipped to it. Checks the span.
    fn intervals(&self) -> Result<Vec<DateRange>, AppError> {
        if self.to < self.from {
            return Err(AppError::invalid("to", "must not be before from"));
        }
        let period = self.interval.period();
        if period.count(self.from, self.to) > MAX_INTERVALS {
            return Err(AppError::invalid(
                "interval",
                format!("gives more than {MAX_INTERVALS} intervals; use a longer one"),
            ));
        }
        Ok(period.ranges(self.from, self.to))
    }

    fn range(&self) -> ReportRange {
        ReportRange {
            from: self.from.to_string(),
            to: self.to.to_string(),
            interval: self.interval.as_str().to_string(),
        }
    }
}

#[derive(Clone)]
pub struct ReportService {
    pool: SqlitePool,
}

impl ReportService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// Formats minor units per currency code, loading each currency once.
struct Formatter {
    currencies: HashMap<String, Currency>,
    primary: String,
}

impl Formatter {
    async fn load(
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        codes: impl IntoIterator<Item = &String>,
    ) -> Result<Self, AppError> {
        let mut currencies = HashMap::new();
        for code in codes {
            let currency = currencies::find(conn, code, "currency").await?;
            currencies.insert(code.clone(), currency);
        }
        let primary = identity::primary_currency(conn, ctx).await?;
        Ok(Self {
            currencies,
            primary,
        })
    }

    fn format(&self, code: &str, minor: i64) -> Result<String, AppError> {
        let currency = self
            .currencies
            .get(code)
            .ok_or_else(|| AppError::Internal(format!("currency {code} not loaded")))?;
        Ok(Money::new(minor, parse_stored(code)?).to_decimal_string(currency))
    }

    /// Sorts currency codes: the primary first, the rest alphabetically.
    fn order(&self, mut codes: Vec<String>) -> Vec<String> {
        codes.sort_by_key(|code| (code != &self.primary, code.clone()));
        codes
    }
}

/// `a - b`, or an error on overflow.
fn minus(a: i64, b: i64) -> Result<i64, AppError> {
    a.checked_sub(b)
        .ok_or_else(|| AppError::Internal("report total too large".to_string()))
}

fn parse_day(text: &str) -> Result<NaiveDate, AppError> {
    parse_stored(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, day).unwrap()
    }

    #[test]
    fn intervals_parse_and_print() {
        for name in ["day", "week", "month", "year"] {
            assert_eq!(Interval::parse(name).unwrap().as_str(), name);
        }
        assert!(Interval::parse("quarter").is_err());
    }

    #[test]
    fn spans_are_checked() {
        let query = |from, to, interval| ReportQuery {
            from,
            to,
            interval,
            scope: Scope::Household,
        };
        assert_eq!(
            query(d(1, 1), d(12, 31), Interval::Month)
                .intervals()
                .unwrap()
                .len(),
            12
        );
        assert!(
            query(d(2, 1), d(1, 1), Interval::Month)
                .intervals()
                .is_err()
        );
        let long = query(
            NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            d(12, 31),
            Interval::Day,
        );
        assert!(
            matches!(long.intervals(), Err(AppError::Invalid { ref field, .. }) if field == "interval")
        );
    }
}
