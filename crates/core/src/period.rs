//! Calendar periods (day, week, month, quarter, half year, year) and the date
//! ranges they cut a span of time into. Used by budgets (auto-budget periods)
//! and reports (intervals).
//!
//! All date maths uses chrono's checked operations, which return `None` on
//! overflow instead of panicking. Unlike Carbon, a `NaiveDate` is a plain
//! value: `date + 1 day` gives a new date and never changes `date` itself.

use chrono::{Datelike, Days, Months, NaiveDate};

/// A calendar period. Weeks start on Monday; quarters on 1 January, April,
/// July and October; half years on 1 January and July.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Period {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    HalfYear,
    Yearly,
}

/// Every period, in the order the API lists them.
pub const PERIODS: [Period; 6] = [
    Period::Daily,
    Period::Weekly,
    Period::Monthly,
    Period::Quarterly,
    Period::HalfYear,
    Period::Yearly,
];

/// An inclusive range of dates: `start <= end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DateRange {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

impl DateRange {
    /// `None` when `end` is before `start`.
    pub fn new(start: NaiveDate, end: NaiveDate) -> Option<Self> {
        (start <= end).then_some(Self { start, end })
    }

    pub fn contains(&self, date: NaiveDate) -> bool {
        self.start <= date && date <= self.end
    }

    pub fn overlaps(&self, other: &DateRange) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}

impl Period {
    /// The stored and API name (`daily`, `weekly`, ..., `half_year`, `yearly`).
    pub fn as_str(self) -> &'static str {
        match self {
            Period::Daily => "daily",
            Period::Weekly => "weekly",
            Period::Monthly => "monthly",
            Period::Quarterly => "quarterly",
            Period::HalfYear => "half_year",
            Period::Yearly => "yearly",
        }
    }

    /// The inverse of [`Period::as_str`].
    pub fn parse(text: &str) -> Option<Self> {
        PERIODS.into_iter().find(|p| p.as_str() == text)
    }

    /// The first day of the period that contains `date`.
    pub fn start_of(self, date: NaiveDate) -> NaiveDate {
        let first_of_month = |month: u32| date.with_day(1).and_then(|d| d.with_month(month));
        let start = match self {
            Period::Daily => Some(date),
            Period::Weekly => {
                let back = u64::from(date.weekday().num_days_from_monday());
                date.checked_sub_days(Days::new(back))
            }
            Period::Monthly => date.with_day(1),
            Period::Quarterly => first_of_month((date.month0() / 3) * 3 + 1),
            Period::HalfYear => first_of_month((date.month0() / 6) * 6 + 1),
            Period::Yearly => first_of_month(1),
        };
        // Day 1 and a valid month always exist; only the far past could fail.
        start.unwrap_or(date)
    }

    /// The first day of the next period, given the first day of a period.
    /// `None` past the last representable date.
    pub fn next_start(self, start: NaiveDate) -> Option<NaiveDate> {
        match self {
            Period::Daily => start.checked_add_days(Days::new(1)),
            Period::Weekly => start.checked_add_days(Days::new(7)),
            Period::Monthly => start.checked_add_months(Months::new(1)),
            Period::Quarterly => start.checked_add_months(Months::new(3)),
            Period::HalfYear => start.checked_add_months(Months::new(6)),
            Period::Yearly => start.checked_add_months(Months::new(12)),
        }
    }

    /// The whole period that contains `date`.
    pub fn range_of(self, date: NaiveDate) -> DateRange {
        let start = self.start_of(date);
        let end = self
            .next_start(start)
            .and_then(|next| next.pred_opt())
            .unwrap_or(NaiveDate::MAX);
        DateRange { start, end }
    }

    /// The periods that overlap `from..=to`, each clipped to that span, in
    /// order. Empty when `to` is before `from`.
    pub fn ranges(self, from: NaiveDate, to: NaiveDate) -> Vec<DateRange> {
        let mut ranges = Vec::new();
        let mut start = from;
        while start <= to {
            let whole = self.range_of(start);
            ranges.push(DateRange {
                start,
                end: whole.end.min(to),
            });
            match whole.end.succ_opt() {
                Some(next) => start = next,
                None => break,
            }
        }
        ranges
    }

    /// How many periods [`Period::ranges`] would return, without building
    /// them (cheap enough to check a limit first).
    pub fn count(self, from: NaiveDate, to: NaiveDate) -> u64 {
        if to < from {
            return 0;
        }
        let months = |a: NaiveDate, b: NaiveDate| {
            let a = i64::from(a.year()) * 12 + i64::from(a.month0());
            let b = i64::from(b.year()) * 12 + i64::from(b.month0());
            b - a
        };
        let (first, last) = (self.start_of(from), self.start_of(to));
        let steps = match self {
            Period::Daily => (last - first).num_days(),
            Period::Weekly => (last - first).num_days() / 7,
            Period::Monthly => months(first, last),
            Period::Quarterly => months(first, last) / 3,
            Period::HalfYear => months(first, last) / 6,
            Period::Yearly => months(first, last) / 12,
        };
        u64::try_from(steps).unwrap_or(0) + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn names_round_trip() {
        for period in PERIODS {
            assert_eq!(Period::parse(period.as_str()), Some(period));
        }
        assert_eq!(Period::parse("fortnightly"), None);
    }

    #[test]
    fn start_of_each_period() {
        // 2026-11-13 is a Friday.
        let date = d(2026, 11, 13);
        assert_eq!(Period::Daily.start_of(date), date);
        assert_eq!(Period::Weekly.start_of(date), d(2026, 11, 9));
        assert_eq!(Period::Weekly.start_of(d(2026, 11, 9)), d(2026, 11, 9));
        assert_eq!(Period::Weekly.start_of(d(2026, 11, 15)), d(2026, 11, 9));
        assert_eq!(Period::Monthly.start_of(date), d(2026, 11, 1));
        assert_eq!(Period::Quarterly.start_of(date), d(2026, 10, 1));
        assert_eq!(Period::Quarterly.start_of(d(2026, 3, 31)), d(2026, 1, 1));
        assert_eq!(Period::HalfYear.start_of(date), d(2026, 7, 1));
        assert_eq!(Period::HalfYear.start_of(d(2026, 6, 30)), d(2026, 1, 1));
        assert_eq!(Period::Yearly.start_of(date), d(2026, 1, 1));
    }

    #[test]
    fn whole_periods_end_the_day_before_the_next() {
        let date = d(2024, 2, 10);
        assert_eq!(Period::Monthly.range_of(date).end, d(2024, 2, 29)); // leap year
        assert_eq!(Period::Monthly.range_of(d(2026, 2, 3)).end, d(2026, 2, 28));
        assert_eq!(Period::Weekly.range_of(date).end, d(2024, 2, 11));
        assert_eq!(Period::Quarterly.range_of(date).end, d(2024, 3, 31));
        assert_eq!(Period::HalfYear.range_of(date).end, d(2024, 6, 30));
        assert_eq!(Period::Yearly.range_of(date).end, d(2024, 12, 31));
        assert_eq!(Period::Daily.range_of(date).end, date);
        // The last representable day has no next period.
        assert_eq!(Period::Yearly.range_of(NaiveDate::MAX).end, NaiveDate::MAX);
    }

    #[test]
    fn ranges_are_clipped_to_the_span() {
        let ranges = Period::Monthly.ranges(d(2026, 1, 15), d(2026, 3, 10));
        assert_eq!(
            ranges,
            vec![
                DateRange::new(d(2026, 1, 15), d(2026, 1, 31)).unwrap(),
                DateRange::new(d(2026, 2, 1), d(2026, 2, 28)).unwrap(),
                DateRange::new(d(2026, 3, 1), d(2026, 3, 10)).unwrap(),
            ]
        );
        assert_eq!(Period::Yearly.ranges(d(2026, 5, 1), d(2026, 5, 1)).len(), 1);
        assert!(
            Period::Daily
                .ranges(d(2026, 5, 2), d(2026, 5, 1))
                .is_empty()
        );
        assert_eq!(Period::Daily.ranges(d(2026, 5, 1), d(2026, 5, 7)).len(), 7);
        // Thursday to the next Wednesday crosses one Monday.
        assert_eq!(
            Period::Weekly
                .ranges(d(2026, 11, 12), d(2026, 11, 18))
                .len(),
            2
        );
    }

    #[test]
    fn count_matches_ranges() {
        let spans = [
            (d(2026, 1, 15), d(2026, 3, 10)),
            (d(2025, 12, 31), d(2026, 1, 1)),
            (d(2020, 2, 29), d(2026, 11, 13)),
            (d(2026, 5, 2), d(2026, 5, 1)),
        ];
        for (from, to) in spans {
            for period in PERIODS {
                assert_eq!(
                    period.count(from, to),
                    period.ranges(from, to).len() as u64,
                    "{period:?} {from} {to}"
                );
            }
        }
    }

    #[test]
    fn date_ranges_check_their_order_and_overlap() {
        assert!(DateRange::new(d(2026, 1, 2), d(2026, 1, 1)).is_none());
        let jan = DateRange::new(d(2026, 1, 1), d(2026, 1, 31)).unwrap();
        let feb = DateRange::new(d(2026, 2, 1), d(2026, 2, 28)).unwrap();
        let mid = DateRange::new(d(2026, 1, 20), d(2026, 2, 5)).unwrap();
        assert!(jan.contains(d(2026, 1, 31)) && !jan.contains(d(2026, 2, 1)));
        assert!(!jan.overlaps(&feb));
        assert!(jan.overlaps(&mid) && mid.overlaps(&feb));
    }
}
