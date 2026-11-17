//! Cash-flow forecast: a balance projected forward from planned amounts
//! (ARCHITECTURE.md 4.3). The caller collects the amounts (recurrences,
//! bills, future-dated transactions); this only adds them up.

use chrono::NaiveDate;

use crate::error::MoneyError;
use crate::money::Money;
use crate::period::DateRange;

/// One planned change to the balance: `amount` is positive when money comes
/// in and negative when it goes out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Planned {
    pub date: NaiveDate,
    pub amount: Money,
}

/// The balance on one day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BalancePoint {
    pub date: NaiveDate,
    pub balance: Money,
}

/// A projected balance in one currency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Projection {
    /// The balance at the end of each interval, in the order given.
    pub points: Vec<BalancePoint>,
    /// The lowest balance on any day from the first interval's start on (the
    /// earliest day wins a tie); the starting balance when nothing is planned.
    pub lowest: BalancePoint,
}

/// Projects `start` (the balance on the first interval's first day) through
/// `intervals` (sorted and adjacent, as `Period::ranges` returns them).
/// Planned amounts outside the intervals are ignored; every amount must be in
/// `start`'s currency. With no intervals, `points` is empty and `lowest` is
/// `start` on `NaiveDate::MIN`.
pub fn project(
    start: Money,
    planned: &[Planned],
    intervals: &[DateRange],
) -> Result<Projection, MoneyError> {
    let first_day = intervals.first().map_or(NaiveDate::MIN, |r| r.start);
    let last_day = intervals.last().map_or(NaiveDate::MIN, |r| r.end);
    let mut sorted: Vec<&Planned> = planned
        .iter()
        .filter(|p| first_day <= p.date && p.date <= last_day)
        .collect();
    // A stable sort keeps same-day amounts in the caller's order.
    sorted.sort_by_key(|p| p.date);

    let mut balance = start;
    let mut lowest = BalancePoint {
        date: first_day,
        balance: start,
    };
    let mut points = Vec::with_capacity(intervals.len());
    let mut next = sorted.iter().peekable();
    for range in intervals {
        // `peekable` lets the loop look at the next item without taking it,
        // like reading ahead in a Go slice with an index.
        while let Some(item) = next.next_if(|p| p.date <= range.end) {
            balance = balance.checked_add(item.amount)?;
            // Only the end of a day counts: wait until the next item is on
            // another day before comparing.
            let day_done = next.peek().is_none_or(|p| p.date != item.date);
            if day_done && balance.minor() < lowest.balance.minor() {
                lowest = BalancePoint {
                    date: item.date,
                    balance,
                };
            }
        }
        points.push(BalancePoint {
            date: range.end,
            balance,
        });
    }
    Ok(Projection { points, lowest })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::currency::CurrencyCode;
    use crate::period::Period;

    fn d(m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, day).unwrap()
    }

    fn eur(minor: i64) -> Money {
        Money::new(minor, CurrencyCode::new("EUR").unwrap())
    }

    fn planned(m: u32, day: u32, minor: i64) -> Planned {
        Planned {
            date: d(m, day),
            amount: eur(minor),
        }
    }

    fn point(m: u32, day: u32, minor: i64) -> BalancePoint {
        BalancePoint {
            date: d(m, day),
            balance: eur(minor),
        }
    }

    #[test]
    fn sums_planned_amounts_into_interval_ends() {
        let months =
            Period::Monthly.ranges(d(11, 17), NaiveDate::from_ymd_opt(2027, 1, 31).unwrap());
        let items = [
            planned(12, 1, -80_000), // rent
            planned(11, 27, 250_000),
            planned(12, 24, -10_000),
            planned(11, 20, -5_000),
            planned(11, 16, -999_999), // before the start: ignored
        ];
        let result = project(eur(100_000), &items, &months).unwrap();
        let ends: Vec<_> = result.points.iter().map(|p| p.balance.minor()).collect();
        assert_eq!(ends, vec![345_000, 255_000, 255_000]);
        assert_eq!(result.points[0].date, d(11, 30));
        assert_eq!(result.lowest, point(11, 20, 95_000));
    }

    #[test]
    fn the_lowest_point_is_the_end_of_a_day() {
        let days = Period::Daily.ranges(d(11, 17), d(11, 20));
        // On the 18th the money goes out before it comes in: the day ends at 100.
        let items = [
            planned(11, 18, -500),
            planned(11, 18, 500),
            planned(11, 19, -30),
        ];
        let result = project(eur(100), &items, &days).unwrap();
        assert_eq!(result.lowest, point(11, 19, 70));
        let ends: Vec<_> = result.points.iter().map(|p| p.balance.minor()).collect();
        assert_eq!(ends, vec![100, 100, 70, 70]);
    }

    #[test]
    fn without_planned_amounts_the_start_is_the_lowest() {
        let weeks = Period::Weekly.ranges(d(11, 17), d(11, 30));
        let result = project(eur(-40), &[], &weeks).unwrap();
        assert_eq!(result.lowest, point(11, 17, -40));
        assert_eq!(result.points.len(), 3);
        assert!(result.points.iter().all(|p| p.balance == eur(-40)));
    }

    #[test]
    fn other_currencies_and_overflow_are_errors() {
        let days = Period::Daily.ranges(d(11, 17), d(11, 18));
        let usd = Planned {
            date: d(11, 18),
            amount: Money::new(1, CurrencyCode::new("USD").unwrap()),
        };
        assert!(matches!(
            project(eur(0), &[usd], &days),
            Err(MoneyError::CurrencyMismatch { .. })
        ));
        let huge = [planned(11, 18, i64::MAX)];
        assert_eq!(project(eur(1), &huge, &days), Err(MoneyError::Overflow));
    }
}
