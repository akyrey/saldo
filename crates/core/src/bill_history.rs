//! A bill's history (Phase 11): which dates the history chart spans, the
//! status of each due date, and a summary of the payments. Pure functions;
//! "today" is a parameter.

use chrono::{Months, NaiveDate};
use serde::Serialize;

use crate::bills::PeriodStatus;
use crate::error::MoneyError;
use crate::ratios::monthly_average;

/// How far past today the history may reach, like the forecast.
pub const MAX_MONTHS_AHEAD: u32 = 24;

/// The dates a bill's history covers, both included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistorySpan {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

/// What the span is computed from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpanInput {
    pub first_due_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    /// The due date of the last `bill_periods` row, if any.
    pub last_period_due: Option<NaiveDate>,
    /// The date of the earliest linked payment, if any.
    pub earliest_payment: Option<NaiveDate>,
    pub today: NaiveDate,
}

/// The span of a bill's history:
/// - from the first due date, or an earlier payment;
/// - to the end date when set, otherwise to the later of today and the last
///   created period;
/// - never past today + [`MAX_MONTHS_AHEAD`] months, and never before `from`.
pub fn history_span(input: SpanInput) -> HistorySpan {
    // `Option::map_or(default, f)`: the PHP `$x === null ? $default : f($x)`.
    let from = input
        .earliest_payment
        .map_or(input.first_due_date, |paid| paid.min(input.first_due_date));
    let natural_end = match input.end_date {
        Some(end) => end,
        None => input
            .last_period_due
            .map_or(input.today, |last| last.max(input.today)),
    };
    let cap = input
        .today
        .checked_add_months(Months::new(MAX_MONTHS_AHEAD))
        .unwrap_or(NaiveDate::MAX);
    HistorySpan {
        from,
        to: natural_end.min(cap).max(from),
    }
}

/// The status of one due date as the history shows it. `Overdue` and
/// `Scheduled` are never stored: they are derived from the stored status,
/// the date and today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DueStatus {
    Paid,
    Skipped,
    Open,
    Overdue,
    /// A due date after the last created period: no row exists yet.
    Scheduled,
}

impl DueStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            DueStatus::Paid => "paid",
            DueStatus::Skipped => "skipped",
            DueStatus::Open => "open",
            DueStatus::Overdue => "overdue",
            DueStatus::Scheduled => "scheduled",
        }
    }
}

/// The status of a due date. `stored` is `None` when there is no period row.
pub fn due_status(stored: Option<PeriodStatus>, due: NaiveDate, today: NaiveDate) -> DueStatus {
    match stored {
        None => DueStatus::Scheduled,
        Some(PeriodStatus::Paid) => DueStatus::Paid,
        Some(PeriodStatus::Skipped) => DueStatus::Skipped,
        Some(PeriodStatus::Open) if due < today => DueStatus::Overdue,
        Some(PeriodStatus::Open) => DueStatus::Open,
    }
}

/// A summary of payment amounts, in minor units of one currency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaymentStats {
    pub count: usize,
    pub total: i64,
    /// Rounded half away from zero.
    pub average: i64,
    pub min: i64,
    pub max: i64,
}

/// Summarises `amounts`; `Ok(None)` when there are none, an error when the
/// total does not fit an `i64`.
pub fn payment_stats(amounts: &[i64]) -> Result<Option<PaymentStats>, MoneyError> {
    let (Some(&min), Some(&max)) = (amounts.iter().min(), amounts.iter().max()) else {
        return Ok(None);
    };
    // `try_fold` stops at the first `None`, like a loop with an early return.
    let total = amounts
        .iter()
        .try_fold(0_i64, |sum, &amount| sum.checked_add(amount))
        .ok_or(MoneyError::Overflow)?;
    Ok(Some(PaymentStats {
        count: amounts.len(),
        total,
        average: monthly_average(amounts),
        min,
        max,
    }))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn d(text: &str) -> NaiveDate {
        text.parse().unwrap()
    }

    fn input(first: &str, today: &str) -> SpanInput {
        SpanInput {
            first_due_date: d(first),
            end_date: None,
            last_period_due: None,
            earliest_payment: None,
            today: d(today),
        }
    }

    #[test]
    fn span_without_end_date_runs_to_the_last_period() {
        let span = history_span(SpanInput {
            last_period_due: Some(d("2026-12-15")),
            ..input("2024-01-15", "2026-11-20")
        });
        assert_eq!((span.from, span.to), (d("2024-01-15"), d("2026-12-15")));
    }

    #[test]
    fn span_without_periods_runs_to_today() {
        let span = history_span(input("2024-01-15", "2026-11-20"));
        assert_eq!(span.to, d("2026-11-20"));
    }

    #[test]
    fn span_runs_to_today_when_the_last_period_is_past() {
        let span = history_span(SpanInput {
            last_period_due: Some(d("2025-03-01")),
            ..input("2024-01-15", "2026-11-20")
        });
        assert_eq!(span.to, d("2026-11-20"));
    }

    #[test]
    fn span_starts_at_an_earlier_payment() {
        let span = history_span(SpanInput {
            earliest_payment: Some(d("2024-01-10")),
            ..input("2024-01-15", "2026-11-20")
        });
        assert_eq!(span.from, d("2024-01-10"));
        let later = history_span(SpanInput {
            earliest_payment: Some(d("2024-02-10")),
            ..input("2024-01-15", "2026-11-20")
        });
        assert_eq!(later.from, d("2024-01-15"));
    }

    #[test]
    fn span_ends_at_a_past_end_date() {
        let span = history_span(SpanInput {
            end_date: Some(d("2025-06-30")),
            last_period_due: Some(d("2025-06-15")),
            ..input("2024-01-15", "2026-11-20")
        });
        assert_eq!(span.to, d("2025-06-30"));
    }

    #[test]
    fn span_is_capped_two_years_ahead() {
        let span = history_span(SpanInput {
            end_date: Some(d("2040-01-01")),
            ..input("2024-01-15", "2026-11-20")
        });
        assert_eq!(span.to, d("2028-11-20"));
    }

    #[test]
    fn span_never_ends_before_it_starts() {
        let span = history_span(input("2030-01-01", "2026-11-20"));
        assert_eq!((span.from, span.to), (d("2030-01-01"), d("2030-01-01")));
    }

    #[test]
    fn due_status_derives_overdue_and_scheduled() {
        let today = d("2026-11-20");
        let cases = [
            (None, "2026-12-01", DueStatus::Scheduled),
            (Some(PeriodStatus::Paid), "2026-11-01", DueStatus::Paid),
            (
                Some(PeriodStatus::Skipped),
                "2026-11-01",
                DueStatus::Skipped,
            ),
            (Some(PeriodStatus::Open), "2026-11-19", DueStatus::Overdue),
            (Some(PeriodStatus::Open), "2026-11-20", DueStatus::Open),
            (Some(PeriodStatus::Open), "2026-12-01", DueStatus::Open),
        ];
        for (stored, due, expected) in cases {
            assert_eq!(due_status(stored, d(due), today), expected, "{due}");
        }
    }

    #[test]
    fn due_status_text() {
        assert_eq!(DueStatus::Paid.as_str(), "paid");
        assert_eq!(DueStatus::Skipped.as_str(), "skipped");
        assert_eq!(DueStatus::Open.as_str(), "open");
        assert_eq!(DueStatus::Overdue.as_str(), "overdue");
        assert_eq!(DueStatus::Scheduled.as_str(), "scheduled");
    }

    #[test]
    fn stats_of_no_payments_is_none() {
        assert_eq!(payment_stats(&[]), Ok(None));
    }

    #[test]
    fn stats_round_the_average_half_away_from_zero() {
        let stats = payment_stats(&[3900, 4250, 4500, 4251]).unwrap().unwrap();
        // 16 901 / 4 = 4 225.25 → 4 225; [1, 2] gives 1.5 → 2.
        assert_eq!(
            stats,
            PaymentStats {
                count: 4,
                total: 16_901,
                average: 4225,
                min: 3900,
                max: 4500
            }
        );
        assert_eq!(payment_stats(&[1, 2]).unwrap().unwrap().average, 2);
    }

    #[test]
    fn stats_report_overflow() {
        assert_eq!(payment_stats(&[i64::MAX, 1]), Err(MoneyError::Overflow));
    }

    proptest! {
        #[test]
        fn span_from_is_never_after_to(
            first in 0_i64..20_000,
            end in proptest::option::of(0_i64..20_000),
            last in proptest::option::of(0_i64..20_000),
            paid in proptest::option::of(0_i64..20_000),
            today in 0_i64..20_000,
        ) {
            let day = |n: i64| d("2000-01-01") + chrono::Days::new(n.unsigned_abs());
            let span = history_span(SpanInput {
                first_due_date: day(first),
                end_date: end.map(day),
                last_period_due: last.map(day),
                earliest_payment: paid.map(day),
                today: day(today),
            });
            prop_assert!(span.from <= span.to);
            prop_assert!(span.from <= day(first));
        }

        #[test]
        fn stats_average_lies_between_min_and_max(
            amounts in proptest::collection::vec(0_i64..1_000_000_000, 1..50),
        ) {
            let stats = payment_stats(&amounts).unwrap().unwrap();
            prop_assert_eq!(stats.count, amounts.len());
            prop_assert!(stats.min <= stats.average && stats.average <= stats.max);
        }
    }
}
