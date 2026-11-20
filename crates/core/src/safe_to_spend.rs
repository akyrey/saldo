//! "Safe to spend this month" (Phase 10): this month's income, received and
//! planned, minus everything spent or committed this month. The app gathers
//! the parts; this module only adds them up and does the date arithmetic.

use chrono::NaiveDate;

use crate::goal::months_until;

/// Every part in minor units of one currency, all non-negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Parts {
    pub income_received: i64,
    pub income_planned: i64,
    pub spent: i64,
    pub bills: i64,
    pub planned: i64,
    pub budgets: i64,
    pub piggy_banks: i64,
    pub savings_transfers: i64,
}

/// Income minus everything spent or set aside; `None` on overflow.
pub fn safe_to_spend(p: &Parts) -> Option<i64> {
    let income = p.income_received.checked_add(p.income_planned)?;
    [
        p.spent,
        p.bills,
        p.planned,
        p.budgets,
        p.piggy_banks,
        p.savings_transfers,
    ]
    .into_iter()
    .try_fold(income, i64::checked_sub)
}

/// Per day left (today counts), rounded towards minus infinity so a
/// shortfall is never understated. `days_left` is at least 1.
pub fn per_day(amount: i64, days_left: i64) -> i64 {
    amount.div_euclid(days_left.max(1))
}

/// What a piggy bank with a target date needs this month: the rest of the
/// target spread over the months left (this one included, rounded up),
/// minus what was already added this month. Never negative.
pub fn piggy_bank_need(
    target: i64,
    saved: i64,
    added_this_month: i64,
    today: NaiveDate,
    target_date: NaiveDate,
) -> i64 {
    let months = months_until(today, target_date);
    // Everything saved before this month is already behind us.
    let rest = target.saturating_sub(saved.saturating_sub(added_this_month));
    if months <= 0 || rest <= 0 {
        return 0;
    }
    let monthly = rest / months + i64::from(rest % months != 0);
    monthly.saturating_sub(added_this_month).max(0)
}

/// The share of `available` that belongs to `[from, month_end]`, when the
/// limit's rest runs `[from, limit_end]`: prorated by days, rounded down.
pub fn prorate(available: i64, from: NaiveDate, month_end: NaiveDate, limit_end: NaiveDate) -> i64 {
    if available <= 0 || limit_end < from {
        return 0;
    }
    let in_month = (month_end.min(limit_end) - from).num_days() + 1;
    let in_limit = (limit_end - from).num_days() + 1;
    let share = i128::from(available) * i128::from(in_month) / i128::from(in_limit);
    i64::try_from(share).unwrap_or(available)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, day).unwrap()
    }

    #[test]
    fn adds_income_and_subtracts_the_rest() {
        let parts = Parts {
            income_received: 200_000,
            income_planned: 10_000,
            spent: 50_000,
            bills: 80_000,
            planned: 5_000,
            budgets: 20_000,
            piggy_banks: 10_000,
            savings_transfers: 15_000,
        };
        assert_eq!(safe_to_spend(&parts), Some(30_000));
        let short = Parts {
            spent: 300_000,
            ..parts
        };
        assert_eq!(safe_to_spend(&short), Some(-220_000));
        let huge = Parts {
            spent: i64::MIN,
            ..Parts::default()
        };
        assert_eq!(safe_to_spend(&huge), None);
    }

    #[test]
    fn per_day_rounds_down_even_below_zero() {
        assert_eq!(per_day(1000, 3), 333);
        assert_eq!(per_day(-1000, 3), -334);
        assert_eq!(per_day(500, 0), 500);
    }

    #[test]
    fn piggy_banks_spread_the_rest_over_the_months_left() {
        // 1200 still to save by end of February: Nov, Dec, Jan, Feb = 4 months.
        let feb = NaiveDate::from_ymd_opt(2027, 2, 28).unwrap();
        assert_eq!(piggy_bank_need(1500, 300, 0, d(11, 20), feb), 300);
        // 100 already added this month counts towards it.
        assert_eq!(piggy_bank_need(1500, 400, 100, d(11, 20), feb), 200);
        // Rounded up: 1000 over 3 months.
        let jan = NaiveDate::from_ymd_opt(2027, 1, 31).unwrap();
        assert_eq!(piggy_bank_need(1000, 0, 0, d(11, 20), jan), 334);
        assert_eq!(piggy_bank_need(1000, 1000, 0, d(11, 20), jan), 0);
        assert_eq!(piggy_bank_need(1000, 0, 0, d(11, 20), d(10, 1)), 0);
    }

    #[test]
    fn prorates_limits_running_past_the_month() {
        // A limit with 300 left until 31 December, from 21 November: 10 of 41 days.
        assert_eq!(prorate(300, d(11, 21), d(11, 30), d(12, 31)), 73);
        // Ends within the month: all of it.
        assert_eq!(prorate(300, d(11, 21), d(11, 30), d(11, 30)), 300);
        assert_eq!(prorate(-5, d(11, 21), d(11, 30), d(11, 30)), 0);
    }
}
