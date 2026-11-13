//! Tests for [`super::occurrences`]: one per rule of the schedule.

use chrono::{NaiveDate, Weekday};
use proptest::prelude::*;

use super::super::{Pattern, RecurrenceSchedule, Repetition, WeekendPolicy};
use super::occurrences;

fn d(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

fn rep(pattern: Pattern) -> Repetition {
    Repetition {
        pattern,
        skip: 0,
        weekend: WeekendPolicy::Keep,
    }
}

fn schedule(first: NaiveDate, repetitions: Vec<Repetition>) -> RecurrenceSchedule {
    RecurrenceSchedule {
        first_date: first,
        repeat_until: None,
        max_occurrences: None,
        repetitions,
    }
}

/// All dates from the first date to `to`.
fn dates(schedule: &RecurrenceSchedule, to: NaiveDate) -> Vec<NaiveDate> {
    occurrences(schedule, schedule.first_date, to)
}

#[test]
fn daily_fires_every_day_from_the_first_date() {
    let s = schedule(d(2026, 11, 13), vec![rep(Pattern::Daily)]);
    assert_eq!(
        dates(&s, d(2026, 11, 16)),
        [
            d(2026, 11, 13),
            d(2026, 11, 14),
            d(2026, 11, 15),
            d(2026, 11, 16)
        ]
    );
}

#[test]
fn skip_keeps_every_nth_slot() {
    let every_third_day = Repetition {
        skip: 2,
        ..rep(Pattern::Daily)
    };
    let s = schedule(d(2026, 11, 1), vec![every_third_day]);
    assert_eq!(
        dates(&s, d(2026, 11, 10)),
        [
            d(2026, 11, 1),
            d(2026, 11, 4),
            d(2026, 11, 7),
            d(2026, 11, 10)
        ]
    );

    let every_other_month = Repetition {
        skip: 1,
        ..rep(Pattern::Monthly(15))
    };
    let s = schedule(d(2026, 1, 20), vec![every_other_month]);
    assert_eq!(
        dates(&s, d(2026, 8, 1)),
        [d(2026, 2, 15), d(2026, 4, 15), d(2026, 6, 15)]
    );
}

#[test]
fn weekly_starts_at_the_first_matching_weekday() {
    // 2026-11-13 is a Friday; the first Monday on or after it is the 16th.
    let s = schedule(d(2026, 11, 13), vec![rep(Pattern::Weekly(Weekday::Mon))]);
    assert_eq!(
        dates(&s, d(2026, 12, 1)),
        [d(2026, 11, 16), d(2026, 11, 23), d(2026, 11, 30)]
    );
    // Same weekday as the first date: that date counts.
    let s = schedule(d(2026, 11, 13), vec![rep(Pattern::Weekly(Weekday::Fri))]);
    assert_eq!(
        dates(&s, d(2026, 11, 20)),
        [d(2026, 11, 13), d(2026, 11, 20)]
    );
    // Every second week.
    let fortnightly = Repetition {
        skip: 1,
        ..rep(Pattern::Weekly(Weekday::Fri))
    };
    let s = schedule(d(2026, 11, 13), vec![fortnightly]);
    assert_eq!(
        dates(&s, d(2026, 12, 11)),
        [d(2026, 11, 13), d(2026, 11, 27), d(2026, 12, 11)]
    );
}

#[test]
fn monthly_clamps_to_the_end_of_short_months() {
    let s = schedule(d(2026, 1, 1), vec![rep(Pattern::Monthly(31))]);
    assert_eq!(
        dates(&s, d(2026, 5, 31)),
        [
            d(2026, 1, 31),
            d(2026, 2, 28),
            d(2026, 3, 31),
            d(2026, 4, 30),
            d(2026, 5, 31)
        ]
    );
}

#[test]
fn monthly_uses_the_leap_day_in_leap_years() {
    let s = schedule(d(2028, 1, 30), vec![rep(Pattern::Monthly(30))]);
    assert_eq!(
        dates(&s, d(2028, 3, 30)),
        [d(2028, 1, 30), d(2028, 2, 29), d(2028, 3, 30)]
    );
}

#[test]
fn monthly_starts_next_month_when_the_day_has_passed() {
    let s = schedule(d(2026, 1, 3), vec![rep(Pattern::Monthly(2))]);
    assert_eq!(dates(&s, d(2026, 3, 31)), [d(2026, 2, 2), d(2026, 3, 2)]);
}

#[test]
fn nth_weekday_picks_the_weekday_of_the_given_week() {
    let second_wednesday = Pattern::NthWeekday {
        week: 2,
        weekday: Weekday::Wed,
    };
    let s = schedule(d(2026, 1, 1), vec![rep(second_wednesday)]);
    assert_eq!(
        dates(&s, d(2026, 3, 31)),
        [d(2026, 1, 14), d(2026, 2, 11), d(2026, 3, 11)]
    );
    // Already past in the first month: start with the next one.
    let s = schedule(d(2026, 1, 15), vec![rep(second_wednesday)]);
    assert_eq!(dates(&s, d(2026, 2, 28)), [d(2026, 2, 11)]);
}

#[test]
fn a_missing_fifth_weekday_becomes_the_last_one() {
    let fifth_friday = Pattern::NthWeekday {
        week: 5,
        weekday: Weekday::Fri,
    };
    let s = schedule(d(2026, 1, 1), vec![rep(fifth_friday)]);
    // January 2026 has five Fridays (30th), February four (27th last).
    assert_eq!(dates(&s, d(2026, 2, 28)), [d(2026, 1, 30), d(2026, 2, 27)]);
}

#[test]
fn yearly_fires_once_a_year_and_clamps_the_leap_day() {
    let s = schedule(
        d(2026, 3, 15),
        vec![rep(Pattern::Yearly { month: 3, day: 15 })],
    );
    assert_eq!(
        dates(&s, d(2028, 12, 31)),
        [d(2026, 3, 15), d(2027, 3, 15), d(2028, 3, 15)]
    );

    let leap_day = Pattern::Yearly { month: 2, day: 29 };
    let s = schedule(d(2027, 1, 1), vec![rep(leap_day)]);
    assert_eq!(
        dates(&s, d(2029, 12, 31)),
        [d(2027, 2, 28), d(2028, 2, 29), d(2029, 2, 28)]
    );

    // The date has passed in the first year: start the next year.
    let s = schedule(
        d(2026, 4, 1),
        vec![rep(Pattern::Yearly { month: 3, day: 15 })],
    );
    assert_eq!(dates(&s, d(2027, 12, 31)), [d(2027, 3, 15)]);

    let every_other_year = Repetition {
        skip: 1,
        ..rep(Pattern::Yearly { month: 3, day: 15 })
    };
    let s = schedule(d(2026, 1, 1), vec![every_other_year]);
    assert_eq!(
        dates(&s, d(2030, 12, 31)),
        [d(2026, 3, 15), d(2028, 3, 15), d(2030, 3, 15)]
    );
}

/// The 1st of August 2026 is a Saturday, of September a Tuesday, of
/// October a Thursday and of November a Sunday.
fn weekend_dates(policy: WeekendPolicy) -> Vec<NaiveDate> {
    let repetition = Repetition {
        weekend: policy,
        ..rep(Pattern::Monthly(1))
    };
    let s = schedule(d(2026, 8, 1), vec![repetition]);
    dates(&s, d(2026, 11, 30))
}

#[test]
fn weekend_policy_keep_leaves_weekend_dates() {
    assert_eq!(
        weekend_dates(WeekendPolicy::Keep),
        [d(2026, 8, 1), d(2026, 9, 1), d(2026, 10, 1), d(2026, 11, 1)]
    );
}

#[test]
fn weekend_policy_skip_drops_weekend_dates() {
    assert_eq!(
        weekend_dates(WeekendPolicy::Skip),
        [d(2026, 9, 1), d(2026, 10, 1)]
    );
}

#[test]
fn weekend_policy_previous_friday_moves_back_but_never_before_the_first_date() {
    // Saturday 08-01 would become Friday 07-31, before the first date: dropped.
    assert_eq!(
        weekend_dates(WeekendPolicy::PreviousFriday),
        [d(2026, 9, 1), d(2026, 10, 1), d(2026, 10, 30)]
    );
}

#[test]
fn weekend_policy_next_monday_moves_forward() {
    assert_eq!(
        weekend_dates(WeekendPolicy::NextMonday),
        [d(2026, 8, 3), d(2026, 9, 1), d(2026, 10, 1), d(2026, 11, 2)]
    );
}

#[test]
fn a_date_moved_past_the_window_is_left_out_and_one_moved_into_it_is_kept() {
    // 2026-11-01 is a Sunday: next Monday is the 2nd, outside a window ending on the 1st.
    let monday = Repetition {
        weekend: WeekendPolicy::NextMonday,
        ..rep(Pattern::Monthly(1))
    };
    let s = schedule(d(2026, 11, 1), vec![monday]);
    assert_eq!(dates(&s, d(2026, 11, 1)), Vec::<NaiveDate>::new());
    // Saturday 2026-08-01 moved to Friday 07-31 is inside a window ending 07-31.
    let friday = Repetition {
        weekend: WeekendPolicy::PreviousFriday,
        ..rep(Pattern::Monthly(1))
    };
    let s = schedule(d(2026, 7, 2), vec![friday]);
    assert_eq!(dates(&s, d(2026, 7, 31)), [d(2026, 7, 31)]);
}

#[test]
fn repeat_until_is_the_last_allowed_date() {
    let s = RecurrenceSchedule {
        repeat_until: Some(d(2026, 11, 15)),
        ..schedule(d(2026, 11, 13), vec![rep(Pattern::Daily)])
    };
    assert_eq!(
        dates(&s, d(2026, 12, 31)),
        [d(2026, 11, 13), d(2026, 11, 14), d(2026, 11, 15)]
    );
}

#[test]
fn max_occurrences_counts_from_the_first_date_not_from_the_window() {
    let s = RecurrenceSchedule {
        max_occurrences: Some(3),
        ..schedule(d(2026, 1, 1), vec![rep(Pattern::Monthly(1))])
    };
    assert_eq!(
        dates(&s, d(2026, 12, 31)),
        [d(2026, 1, 1), d(2026, 2, 1), d(2026, 3, 1)]
    );
    // A window starting after the third date sees nothing.
    assert_eq!(
        occurrences(&s, d(2026, 4, 1), d(2026, 12, 31)),
        Vec::<NaiveDate>::new()
    );
    // A window in the middle sees only what is left.
    assert_eq!(
        occurrences(&s, d(2026, 2, 1), d(2026, 12, 31)),
        [d(2026, 2, 1), d(2026, 3, 1)]
    );
}

#[test]
fn skipped_weekend_dates_do_not_count_towards_max_occurrences() {
    let repetition = Repetition {
        weekend: WeekendPolicy::Skip,
        ..rep(Pattern::Monthly(1))
    };
    let s = RecurrenceSchedule {
        max_occurrences: Some(2),
        ..schedule(d(2026, 8, 1), vec![repetition])
    };
    // August (Sat) and November (Sun) are dropped; September and October count.
    assert_eq!(dates(&s, d(2027, 1, 31)), [d(2026, 9, 1), d(2026, 10, 1)]);
}

#[test]
fn several_repetitions_merge_in_date_order_without_duplicates() {
    let s = schedule(
        d(2026, 11, 1),
        vec![
            rep(Pattern::Monthly(15)),
            rep(Pattern::Monthly(1)),
            rep(Pattern::Monthly(15)),
        ],
    );
    assert_eq!(
        dates(&s, d(2026, 12, 15)),
        [
            d(2026, 11, 1),
            d(2026, 11, 15),
            d(2026, 12, 1),
            d(2026, 12, 15)
        ]
    );
}

#[test]
fn empty_schedules_and_reversed_windows_yield_nothing() {
    let s = schedule(d(2026, 11, 1), vec![]);
    assert!(dates(&s, d(2027, 1, 1)).is_empty());
    let s = schedule(d(2026, 11, 1), vec![rep(Pattern::Daily)]);
    assert!(occurrences(&s, d(2026, 11, 5), d(2026, 11, 4)).is_empty());
    // A window ending before the first date.
    assert!(occurrences(&s, d(2026, 1, 1), d(2026, 10, 31)).is_empty());
}

#[test]
fn far_future_dates_do_not_overflow() {
    let s = schedule(
        NaiveDate::MAX,
        vec![rep(Pattern::Monthly(31)), rep(Pattern::Daily)],
    );
    assert_eq!(dates(&s, NaiveDate::MAX), [NaiveDate::MAX]);
}

proptest! {
    /// Every date lies in the window and in order, and splitting the window
    /// in two gives the same dates as asking for it whole.
    #[test]
    fn windows_compose(
        day in 1u32..=31,
        skip in 0u32..3,
        offset in 0u64..400,
        split in 0u64..400,
    ) {
        let first = d(2026, 1, 1);
        let s = schedule(first, vec![Repetition { skip, ..rep(Pattern::Monthly(day)) }]);
        let to = first + chrono::Days::new(offset + split);
        let mid = first + chrono::Days::new(split);
        let whole = occurrences(&s, first, to);
        prop_assert!(whole.windows(2).all(|w| w[0] < w[1]));
        prop_assert!(whole.iter().all(|date| *date >= first && *date <= to));
        let mut parts = occurrences(&s, first, mid);
        parts.extend(occurrences(&s, mid + chrono::Days::new(1), to));
        prop_assert_eq!(whole, parts);
    }
}
