//! The dates a recurrence fires on.
//!
//! Each repetition yields an endless, *lazy* sequence of raw dates (an
//! `impl Iterator`): nothing is computed until someone asks for the next item,
//! like a PHP generator using `yield` or a Go `iter.Seq`. [`occurrences`]
//! takes raw dates up to the end of the window, applies the weekend policy,
//! merges the repetitions and then applies `repeat_until`, `max_occurrences`
//! and the `from` bound.

use std::collections::BTreeSet;

use chrono::{Datelike, Days, Months, NaiveDate, Weekday};

use super::{Pattern, RecurrenceSchedule, Repetition, WeekendPolicy};

/// A weekend date moves by at most two days, so raw dates up to two days
/// after the window can still land inside it.
const WEEKEND_SHIFT_DAYS: u64 = 2;

/// The dates in `from..=to` (both inclusive) on which the recurrence fires,
/// in order and without duplicates. Pure: "today" is whatever the caller
/// passes as `to`.
pub fn occurrences(
    schedule: &RecurrenceSchedule,
    from: NaiveDate,
    to: NaiveDate,
) -> Vec<NaiveDate> {
    let last = match schedule.repeat_until {
        Some(until) => to.min(until),
        None => to,
    };
    let horizon = last
        .checked_add_days(Days::new(WEEKEND_SHIFT_DAYS))
        .unwrap_or(last);
    // A `BTreeSet` keeps its items sorted and unique: the merged dates of
    // every repetition, in date order.
    let merged: BTreeSet<NaiveDate> = schedule
        .repetitions
        .iter()
        .flat_map(|repetition| {
            raw_dates(repetition, schedule.first_date)
                .take_while(move |date| *date <= horizon)
                .filter_map(move |date| on_weekend(date, repetition.weekend))
        })
        // A weekend shift never moves a date before the recurrence starts.
        .filter(|date| *date >= schedule.first_date && *date <= last)
        .collect();
    let limit = schedule
        .max_occurrences
        .map_or(usize::MAX, |max| usize::try_from(max).unwrap_or(usize::MAX));
    merged
        .into_iter()
        .take(limit)
        .filter(|date| *date >= from)
        .collect()
}

/// The raw dates of one repetition from `first` on, before the weekend
/// policy. Every `skip + 1`-th slot is kept. The sequence ends early only if
/// the date maths overflows (far beyond year 200,000).
fn raw_dates(repetition: &Repetition, first: NaiveDate) -> impl Iterator<Item = NaiveDate> {
    let step = u64::from(repetition.skip) + 1;
    let pattern = repetition.pattern;
    let start = first_slot(pattern, first);
    // `(0u64..)` counts forever; `map_while` stops at the first `None`.
    // `move` copies `pattern`, `start` and `step` into the closure, which then
    // owns them: the iterator outlives this function call.
    (0u64..).map_while(move |n| {
        let start = start?;
        let offset = n.checked_mul(step)?;
        slot(pattern, start, offset)
    })
}

/// The first raw date on or after `first`.
fn first_slot(pattern: Pattern, first: NaiveDate) -> Option<NaiveDate> {
    match pattern {
        Pattern::Daily => Some(first),
        Pattern::Weekly(weekday) => {
            let ahead =
                (7 + weekday.num_days_from_monday() - first.weekday().num_days_from_monday()) % 7;
            first.checked_add_days(Days::new(u64::from(ahead)))
        }
        // For calendar patterns the "slot" is the month or year itself:
        // start with the one holding `first`, or the next if its date is earlier.
        Pattern::Monthly(_) | Pattern::NthWeekday { .. } => {
            let month = first.with_day(1)?;
            let date = in_month(pattern, month)?;
            if date >= first {
                Some(month)
            } else {
                month.checked_add_months(Months::new(1))
            }
        }
        Pattern::Yearly { .. } => {
            let year = NaiveDate::from_ymd_opt(first.year(), 1, 1)?;
            let date = in_month(pattern, year)?;
            if date >= first {
                Some(year)
            } else {
                NaiveDate::from_ymd_opt(first.year().checked_add(1)?, 1, 1)
            }
        }
    }
}

/// The raw date `offset` slots after `start` (see [`first_slot`]).
fn slot(pattern: Pattern, start: NaiveDate, offset: u64) -> Option<NaiveDate> {
    match pattern {
        Pattern::Daily => start.checked_add_days(Days::new(offset)),
        Pattern::Weekly(_) => start.checked_add_days(Days::new(offset.checked_mul(7)?)),
        Pattern::Monthly(_) | Pattern::NthWeekday { .. } => {
            let month = start.checked_add_months(Months::new(u32::try_from(offset).ok()?))?;
            in_month(pattern, month)
        }
        Pattern::Yearly { .. } => {
            let months = u32::try_from(offset.checked_mul(12)?).ok()?;
            in_month(pattern, start.checked_add_months(Months::new(months))?)
        }
    }
}

/// The pattern's date in the month starting at `month` (for yearly
/// patterns, `month` is 1 January of the year).
fn in_month(pattern: Pattern, month: NaiveDate) -> Option<NaiveDate> {
    match pattern {
        Pattern::Monthly(day) => clamped(month.year(), month.month(), day),
        Pattern::NthWeekday { week, weekday } => nth_weekday(month, week, weekday),
        Pattern::Yearly { month: m, day } => clamped(month.year(), m, day),
        Pattern::Daily | Pattern::Weekly(_) => Some(month),
    }
}

/// `day` of the month, or the month's last day if it is shorter
/// (31 → 30 in April, → 28 or 29 in February).
fn clamped(year: i32, month: u32, day: u32) -> Option<NaiveDate> {
    let first = NaiveDate::from_ymd_opt(year, month, 1)?;
    let last_day = first.checked_add_months(Months::new(1))?.pred_opt()?.day();
    first.with_day(day.min(last_day))
}

/// The `week`-th `weekday` of the month starting at `month`; the last one
/// if the month has fewer.
fn nth_weekday(month: NaiveDate, week: u32, weekday: Weekday) -> Option<NaiveDate> {
    let wanted = u8::try_from(week).ok()?;
    NaiveDate::from_weekday_of_month_opt(month.year(), month.month(), weekday, wanted).or_else(
        || {
            // Fewer than `week`: the fourth is always there, so try it.
            NaiveDate::from_weekday_of_month_opt(month.year(), month.month(), weekday, 4)
        },
    )
}

/// Applies the weekend policy; `None` drops the date.
fn on_weekend(date: NaiveDate, policy: WeekendPolicy) -> Option<NaiveDate> {
    // Pattern matching on a tuple: each arm checks both values at once.
    match (date.weekday(), policy) {
        (Weekday::Sat | Weekday::Sun, WeekendPolicy::Skip) => None,
        (Weekday::Sat, WeekendPolicy::PreviousFriday) => date.checked_sub_days(Days::new(1)),
        (Weekday::Sun, WeekendPolicy::PreviousFriday) => date.checked_sub_days(Days::new(2)),
        (Weekday::Sat, WeekendPolicy::NextMonday) => date.checked_add_days(Days::new(2)),
        (Weekday::Sun, WeekendPolicy::NextMonday) => date.checked_add_days(Days::new(1)),
        _ => Some(date),
    }
}

#[cfg(test)]
#[path = "schedule_tests.rs"]
mod tests;
