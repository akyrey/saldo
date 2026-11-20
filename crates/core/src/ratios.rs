//! Ratios and averages of money (Phase 10 reports), in integers only: a
//! share is in basis points (1 bp = 0.01%, so 10 000 bp = 100%), and every
//! division rounds half away from zero, like `round()` in PHP.

/// Basis points in a whole.
pub const BP_PER_WHOLE: i64 = 10_000;

/// `n / d` rounded half away from zero; `d` must not be zero.
fn div_round(n: i128, d: i128) -> i128 {
    let (q, r) = (n / d, n % d);
    // Compare twice the remainder with the divisor, both made positive.
    if 2 * r.abs() >= d.abs() {
        q + if (n < 0) == (d < 0) { 1 } else { -1 }
    } else {
        q
    }
}

/// `part / whole` in basis points; `None` when `whole` is zero.
pub fn ratio_bp(part: i64, whole: i64) -> Option<i64> {
    if whole == 0 {
        return None;
    }
    let bp = div_round(
        i128::from(part) * i128::from(BP_PER_WHOLE),
        i128::from(whole),
    );
    i64::try_from(bp).ok()
}

/// The share of income not spent: `(income - expenses) / income`. Negative
/// when spending was larger; `None` without income.
pub fn savings_rate_bp(income: i64, expenses: i64) -> Option<i64> {
    if income <= 0 {
        return None;
    }
    ratio_bp(income.checked_sub(expenses)?, income)
}

/// How much `now` differs from `before`, relative to `before`:
/// `(now - before) / |before|`. `None` when `before` is zero.
pub fn change_bp(now: i64, before: i64) -> Option<i64> {
    ratio_bp(now.checked_sub(before)?, before.checked_abs()?)
}

/// The mean of monthly totals, rounded half away from zero; zero for none.
pub fn monthly_average(totals: &[i64]) -> i64 {
    if totals.is_empty() {
        return 0;
    }
    let sum: i128 = totals.iter().map(|&t| i128::from(t)).sum();
    let count = i128::try_from(totals.len()).unwrap_or(i128::MAX);
    // The mean of i64 values always fits an i64.
    i64::try_from(div_round(sum, count)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn rounding_is_half_away_from_zero() {
        assert_eq!(div_round(5, 2), 3);
        assert_eq!(div_round(-5, 2), -3);
        assert_eq!(div_round(4, 3), 1);
        assert_eq!(div_round(-4, 3), -1);
        assert_eq!(div_round(7, -2), -4);
    }

    #[test]
    fn savings_rate() {
        assert_eq!(savings_rate_bp(200_000, 150_000), Some(2_500));
        assert_eq!(savings_rate_bp(200_000, 250_000), Some(-2_500));
        assert_eq!(savings_rate_bp(300, 100), Some(6_667));
        assert_eq!(savings_rate_bp(0, 100), None);
        assert_eq!(savings_rate_bp(100, 0), Some(BP_PER_WHOLE));
    }

    #[test]
    fn change_against_the_size_of_before() {
        assert_eq!(change_bp(120, 100), Some(2_000));
        assert_eq!(change_bp(80, 100), Some(-2_000));
        assert_eq!(
            change_bp(-50, -100),
            Some(5_000),
            "a smaller loss is a rise"
        );
        assert_eq!(change_bp(10, 0), None);
    }

    #[test]
    fn averages_round_and_count_zero_months() {
        assert_eq!(monthly_average(&[]), 0);
        assert_eq!(monthly_average(&[100, 0, 0]), 33);
        assert_eq!(monthly_average(&[100, 0, 1]), 34);
        assert_eq!(monthly_average(&[i64::MAX, i64::MAX]), i64::MAX);
    }

    proptest! {
        #[test]
        fn the_average_lies_between_the_smallest_and_largest(
            totals in proptest::collection::vec(-1_000_000_000i64..1_000_000_000, 1..30)
        ) {
            let average = monthly_average(&totals);
            prop_assert!(average >= *totals.iter().min().unwrap());
            prop_assert!(average <= *totals.iter().max().unwrap());
        }

        #[test]
        fn a_ratio_of_whole_is_ten_thousand(whole in 1i64..1_000_000_000_000) {
            prop_assert_eq!(ratio_bp(whole, whole), Some(BP_PER_WHOLE));
        }
    }
}
