//! Folding SQL rows (one per day) into intervals: the shared engine of the
//! cash-flow, category, tag and budget reports.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use saldo_core::DateRange;

use crate::error::AppError;

/// One row from SQL: a day, a currency, what it is grouped by (a category,
/// tag or budget; `None` for "without") and two sums (income and expenses,
/// or budgeted and spent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DayRow {
    pub date: NaiveDate,
    pub currency: String,
    pub key: Option<String>,
    pub name: Option<String>,
    pub first: i64,
    pub second: i64,
}

/// The sums of one group in one currency: per interval and in total.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Series {
    pub name: Option<String>,
    pub intervals: Vec<(i64, i64)>,
    pub total: (i64, i64),
}

/// Currency → group key → series.
pub(crate) type Grouped = BTreeMap<String, BTreeMap<Option<String>, Series>>;

/// The index of the interval containing `date`; `None` outside them all.
/// `partition_point` is a binary search: the intervals are sorted.
pub(crate) fn interval_of(intervals: &[DateRange], date: NaiveDate) -> Option<usize> {
    let index = intervals.partition_point(|r| r.end < date);
    intervals
        .get(index)
        .filter(|r| r.contains(date))
        .map(|_| index)
}

/// Adds every row to its currency, group and interval. Rows outside the
/// intervals are ignored. Sums use checked arithmetic: an overflow is an
/// error, never a wrong total.
pub(crate) fn fold(intervals: &[DateRange], rows: Vec<DayRow>) -> Result<Grouped, AppError> {
    let overflow = || AppError::Internal("report total too large".to_string());
    // `checked_add` returns `None` on overflow; `ok_or_else` takes a closure
    // that only runs in that case (an `FnOnce`: called at most once).
    let add = |a: i64, b: i64| a.checked_add(b).ok_or_else(overflow);
    let mut grouped = Grouped::new();
    for row in rows {
        let Some(index) = interval_of(intervals, row.date) else {
            continue;
        };
        let series = grouped
            .entry(row.currency)
            .or_default()
            .entry(row.key)
            .or_insert_with(|| Series {
                name: row.name.clone(),
                intervals: vec![(0, 0); intervals.len()],
                total: (0, 0),
            });
        let slot = &mut series.intervals[index];
        *slot = (add(slot.0, row.first)?, add(slot.1, row.second)?);
        series.total = (
            add(series.total.0, row.first)?,
            add(series.total.1, row.second)?,
        );
    }
    Ok(grouped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use saldo_core::Period;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap()
    }

    fn row(day: u32, currency: &str, key: Option<&str>, first: i64, second: i64) -> DayRow {
        DayRow {
            date: d(day),
            currency: currency.into(),
            key: key.map(str::to_string),
            name: key.map(|k| format!("name {k}")),
            first,
            second,
        }
    }

    #[test]
    fn finds_the_interval_of_a_date() {
        let weeks = Period::Weekly.ranges(d(4), d(20));
        assert_eq!(interval_of(&weeks, d(4)), Some(0));
        assert_eq!(interval_of(&weeks, d(8)), Some(0)); // Sunday
        assert_eq!(interval_of(&weeks, d(9)), Some(1));
        assert_eq!(interval_of(&weeks, d(20)), Some(2));
        assert_eq!(interval_of(&weeks, d(3)), None);
        assert_eq!(interval_of(&weeks, d(21)), None);
        assert_eq!(interval_of(&[], d(1)), None);
    }

    #[test]
    fn folds_rows_by_currency_group_and_interval() {
        let weeks = Period::Weekly.ranges(d(2), d(15));
        let rows = vec![
            row(2, "EUR", Some("a"), 100, 1),
            row(3, "EUR", Some("a"), 50, 0),
            row(10, "EUR", Some("a"), 0, 7),
            row(10, "EUR", None, 5, 5),
            row(10, "USD", Some("a"), 9, 9),
            row(30, "EUR", Some("a"), 1_000, 1_000), // outside
        ];
        let grouped = fold(&weeks, rows).unwrap();
        let a = &grouped["EUR"][&Some("a".to_string())];
        assert_eq!(a.intervals, vec![(150, 1), (0, 7)]);
        assert_eq!(a.total, (150, 8));
        assert_eq!(a.name.as_deref(), Some("name a"));
        assert_eq!(grouped["EUR"][&None].total, (5, 5));
        assert_eq!(grouped["USD"][&Some("a".to_string())].total, (9, 9));
    }

    #[test]
    fn overflow_is_an_error() {
        let days = Period::Daily.ranges(d(1), d(2));
        let rows = vec![row(1, "EUR", None, i64::MAX, 0), row(2, "EUR", None, 1, 0)];
        assert!(matches!(fold(&days, rows), Err(AppError::Internal(_))));
    }
}
