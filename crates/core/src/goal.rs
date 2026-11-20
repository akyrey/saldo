//! Net worth goals (Phase 10): how far a goal has come, what it still needs
//! each month, and when the recent trend would reach it. A goal may aim
//! down as well as up (paying debt off to zero: start -5000, target 0).

use chrono::{Datelike, Months, NaiveDate};

use crate::ratios::{BP_PER_WHOLE, ratio_bp};

/// What [`progress`] works out; amounts are minor units of the goal's currency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoalProgress {
    /// Still to go, in the goal's direction (zero once reached).
    pub remaining: i64,
    /// From start (0) to target (10 000); clamped to that range.
    pub progress_bp: i64,
    /// What each month until the target date must add (rounded up);
    /// `None` without a target date or once it has passed.
    pub required_monthly: Option<i64>,
    /// When the average monthly change would reach the target; `None` when
    /// that change does not point towards it.
    pub projected_date: Option<NaiveDate>,
    /// Reached, or projected to arrive by the target date (with no target
    /// date: moving the right way).
    pub on_track: bool,
    pub reached: bool,
}

/// Whether `current` has reached `target`, coming from `start`.
pub fn reached(start: i64, current: i64, target: i64) -> bool {
    if target >= start {
        current >= target
    } else {
        current <= target
    }
}

/// Calendar months from `today`'s month to `target`'s, counting both:
/// the months still available to save in. Zero when `target` is earlier.
pub fn months_until(today: NaiveDate, target: NaiveDate) -> i64 {
    let months = (i64::from(target.year()) - i64::from(today.year())) * 12
        + i64::from(target.month())
        - i64::from(today.month());
    if target < today { 0 } else { months + 1 }
}

/// `n / d` rounded up for positive values; `d` must be positive.
fn div_ceil(n: i128, d: i128) -> i128 {
    let q = n / d;
    if n % d > 0 { q + 1 } else { q }
}

pub fn progress(
    start: i64,
    current: i64,
    target: i64,
    today: NaiveDate,
    target_date: Option<NaiveDate>,
    avg_monthly_change: i64,
) -> GoalProgress {
    let up = target >= start;
    // Distances measured in the goal's direction, so "down" goals work alike.
    let (gap, done, pace) = if up {
        (
            i128::from(target) - i128::from(start),
            i128::from(current) - i128::from(start),
            i128::from(avg_monthly_change),
        )
    } else {
        (
            i128::from(start) - i128::from(target),
            i128::from(start) - i128::from(current),
            -i128::from(avg_monthly_change),
        )
    };
    let remaining_wide = (gap - done).max(0);
    let remaining = i64::try_from(remaining_wide).unwrap_or(i64::MAX);
    let is_reached = reached(start, current, target);
    let progress_bp = if gap == 0 {
        BP_PER_WHOLE
    } else {
        i64::try_from(done)
            .ok()
            .zip(i64::try_from(gap).ok())
            .and_then(|(d, g)| ratio_bp(d, g))
            .unwrap_or(0)
            .clamp(0, BP_PER_WHOLE)
    };
    let months_left = target_date.map(|date| months_until(today, date));
    let required_monthly = match months_left {
        Some(left) if left > 0 => i64::try_from(div_ceil(remaining_wide, i128::from(left))).ok(),
        _ => None,
    };
    let projected_date = if is_reached {
        Some(today)
    } else if pace > 0 {
        u32::try_from(div_ceil(remaining_wide, pace))
            .ok()
            .and_then(|months| today.checked_add_months(Months::new(months)))
    } else {
        None
    };
    let on_track = is_reached
        || match (target_date, projected_date) {
            (Some(due), Some(projected)) => projected <= due,
            (Some(_), None) => false,
            (None, _) => pace > 0,
        };
    GoalProgress {
        remaining,
        progress_bp,
        required_monthly,
        projected_date,
        on_track,
        reached: is_reached,
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn saving_up_towards_a_dated_target() {
        // From 1000 to 10000 by June 2027, at 4000 now, saving 500 a month.
        let p = progress(
            1000,
            4000,
            10000,
            d(2026, 11, 20),
            Some(d(2027, 6, 30)),
            500,
        );
        assert_eq!(p.remaining, 6000);
        assert_eq!(p.progress_bp, 3333);
        // November to June is 8 months: 6000 / 8 = 750.
        assert_eq!(p.required_monthly, Some(750));
        // 6000 / 500 = 12 months.
        assert_eq!(p.projected_date, Some(d(2027, 11, 20)));
        assert!(!p.on_track && !p.reached);
    }

    #[test]
    fn paying_debt_down_to_zero() {
        let p = progress(-5000, -2000, 0, d(2026, 11, 20), None, 1000);
        assert_eq!(p.remaining, 2000);
        assert_eq!(p.progress_bp, 6000);
        assert_eq!(p.projected_date, Some(d(2027, 1, 20)));
        assert!(p.on_track);
        let reached_it = progress(-5000, 10, 0, d(2026, 11, 20), None, 0);
        assert!(reached_it.reached && reached_it.on_track);
        assert_eq!((reached_it.remaining, reached_it.progress_bp), (0, 10000));
    }

    #[test]
    fn a_goal_moving_away_has_no_projection() {
        let p = progress(0, -100, 1000, d(2026, 11, 20), Some(d(2027, 1, 1)), -50);
        assert_eq!(p.progress_bp, 0);
        assert_eq!(p.projected_date, None);
        assert!(!p.on_track);
    }

    #[test]
    fn months_until_counts_both_ends() {
        assert_eq!(months_until(d(2026, 11, 20), d(2026, 11, 30)), 1);
        assert_eq!(months_until(d(2026, 11, 20), d(2027, 1, 1)), 3);
        assert_eq!(months_until(d(2026, 11, 20), d(2026, 10, 31)), 0);
        let past = progress(0, 10, 100, d(2026, 11, 20), Some(d(2026, 1, 1)), 10);
        assert_eq!(past.required_monthly, None);
    }

    proptest! {
        #[test]
        fn progress_stays_between_zero_and_whole(
            start in -1_000_000i64..1_000_000,
            current in -2_000_000i64..2_000_000,
            target in -1_000_000i64..1_000_000,
            pace in -10_000i64..10_000,
        ) {
            let p = progress(start, current, target, d(2026, 11, 20), Some(d(2028, 1, 1)), pace);
            prop_assert!((0..=BP_PER_WHOLE).contains(&p.progress_bp));
            prop_assert!(p.remaining >= 0);
        }

        #[test]
        fn the_required_monthly_amount_covers_what_remains(
            start in 0i64..1_000_000,
            gap in 1i64..1_000_000,
            done in 0i64..1_000_000,
            months in 1u32..60,
        ) {
            let today = d(2026, 11, 20);
            let due = today.checked_add_months(Months::new(months - 1)).unwrap();
            let p = progress(start, start + done, start + gap, today, Some(due), 0);
            let left = months_until(today, due);
            prop_assert!(p.required_monthly.unwrap() * left >= p.remaining);
        }
    }
}
