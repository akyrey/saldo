//! Date windows for sources the bank books after the fact (L9, decision 10).

use chrono::NaiveDate;

/// Each day a bank line comes *before* its source row costs this many days:
/// it happens (time zones, weekends) but less often than after.
pub const EARLY_PENALTY: i64 = 2;

/// How far a bank line may be from its source row, as `bank date − source
/// date` in days: from `-early_days` to `late_days`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LagWindow {
    pub early_days: u32,
    pub late_days: u32,
}

impl LagWindow {
    /// The weighted distance of a bank line from its source row, or `None`
    /// when it is outside the window.
    pub fn distance(&self, bank: NaiveDate, source: NaiveDate) -> Option<i64> {
        let lag = (bank - source).num_days();
        if lag < -i64::from(self.early_days) || lag > i64::from(self.late_days) {
            return None;
        }
        Some(if lag < 0 { -lag * EARLY_PENALTY } else { lag })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::date;

    const WINDOW: LagWindow = LagWindow {
        early_days: 2,
        late_days: 7,
    };

    #[test]
    fn days_after_count_once_and_days_before_twice() {
        assert_eq!(WINDOW.distance(date(10), date(10)), Some(0));
        assert_eq!(WINDOW.distance(date(13), date(10)), Some(3));
        assert_eq!(WINDOW.distance(date(9), date(10)), Some(2));
        assert_eq!(WINDOW.distance(date(8), date(10)), Some(4));
    }

    #[test]
    fn outside_the_window_is_none() {
        assert_eq!(WINDOW.distance(date(7), date(10)), None);
        assert_eq!(WINDOW.distance(date(18), date(10)), None);
        assert_eq!(WINDOW.distance(date(17), date(10)), Some(7));
    }
}
