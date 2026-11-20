//! Alerts (Phase 10): when a budget crosses a threshold, and when the weekly
//! digest is due. An alert fires once per crossing: the app keeps the set of
//! conditions that hold, and [`diff`] says which are new (alert) and which
//! stopped (forget, so a later crossing alerts again).

use std::collections::BTreeSet;

use chrono::{Datelike, Days, NaiveDate, NaiveDateTime, NaiveTime, Weekday};

/// The shares of a budget (in percent) that alert once spent.
pub const BUDGET_THRESHOLDS: [u8; 2] = [80, 100];

/// The local hour on Monday from which the weekly digest goes out.
pub const DIGEST_HOUR: u32 = 8;

/// The thresholds reached when `spent` of `total` (both minor units of one
/// currency) is gone. A budget with nothing left to spend (`total <= 0`, after
/// carrying overspending) has reached every threshold as soon as anything is
/// spent. Integer maths only: `spent * 100 >= total * threshold`.
pub fn budget_thresholds(total: i64, spent: i64) -> Vec<u8> {
    if spent <= 0 {
        return Vec::new();
    }
    BUDGET_THRESHOLDS
        .into_iter()
        .filter(|&threshold| {
            // i128 so that even i64::MAX amounts cannot overflow.
            total <= 0 || i128::from(spent) * 100 >= i128::from(total) * i128::from(threshold)
        })
        .collect()
}

/// Compares the conditions that held (`held`) with those that hold `now`:
/// returns `(started, stopped)`, each sorted. A `BTreeSet` is a sorted set
/// (like a Go `map[string]struct{}` that also keeps its keys in order), so
/// the alerts come out in a stable order.
pub fn diff(held: &BTreeSet<String>, now: &BTreeSet<String>) -> (Vec<String>, Vec<String>) {
    let started = now.difference(held).cloned().collect();
    let stopped = held.difference(now).cloned().collect();
    (started, stopped)
}

/// The week a digest covers, Monday to Sunday.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DigestWeek {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

/// The key of this week's digest, `digest:<ISO year>-W<week>`, and the week
/// it reports on (the previous one), once it is Monday 08:00 or later in the
/// household's time. `None` before that on a Monday.
pub fn digest_due(now_local: NaiveDateTime) -> Option<(String, DigestWeek)> {
    let today = now_local.date();
    let monday = today - Days::new(u64::from(today.weekday().num_days_from_monday()));
    let opens = monday.and_time(NaiveTime::from_hms_opt(DIGEST_HOUR, 0, 0)?);
    if now_local < opens {
        return None;
    }
    let week = monday.iso_week();
    let key = format!("digest:{}-W{:02}", week.year(), week.week());
    let from = monday - Days::new(7);
    let to = monday - Days::new(1);
    debug_assert_eq!(from.weekday(), Weekday::Mon);
    Some((key, DigestWeek { from, to }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(keys: &[&str]) -> BTreeSet<String> {
        keys.iter().map(|k| k.to_string()).collect()
    }

    fn at(day: u32, hour: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, day)
            .unwrap()
            .and_hms_opt(hour, 0, 0)
            .unwrap()
    }

    #[test]
    fn thresholds_follow_the_share_spent() {
        assert!(budget_thresholds(10000, 0).is_empty());
        assert!(budget_thresholds(10000, 7999).is_empty());
        assert_eq!(budget_thresholds(10000, 8000), [80]);
        assert_eq!(budget_thresholds(10000, 9999), [80]);
        assert_eq!(budget_thresholds(10000, 10000), [80, 100]);
        assert_eq!(budget_thresholds(10000, 25000), [80, 100]);
    }

    #[test]
    fn nothing_left_to_spend_reaches_every_threshold_on_any_spend() {
        assert_eq!(budget_thresholds(0, 1), [80, 100]);
        assert_eq!(budget_thresholds(-5000, 1), [80, 100]);
        assert!(budget_thresholds(-5000, 0).is_empty());
    }

    #[test]
    fn huge_amounts_do_not_overflow() {
        assert_eq!(budget_thresholds(i64::MAX, i64::MAX), [80, 100]);
    }

    #[test]
    fn diff_splits_started_and_stopped() {
        let (started, stopped) = diff(&set(&["a", "b"]), &set(&["b", "c", "d"]));
        assert_eq!(started, ["c", "d"]);
        assert_eq!(stopped, ["a"]);
        let (started, stopped) = diff(&set(&["a"]), &set(&["a"]));
        assert!(started.is_empty() && stopped.is_empty());
    }

    #[test]
    fn digest_opens_monday_at_eight_and_covers_last_week() {
        // 2026-11-16 is a Monday (ISO week 47).
        assert_eq!(digest_due(at(16, 7)), None);
        let (key, week) = digest_due(at(16, 8)).unwrap();
        assert_eq!(key, "digest:2026-W47");
        assert_eq!(week.from, NaiveDate::from_ymd_opt(2026, 11, 9).unwrap());
        assert_eq!(week.to, NaiveDate::from_ymd_opt(2026, 11, 15).unwrap());
        // Later in the same week: same key, so it is sent only once.
        assert_eq!(digest_due(at(19, 3)).unwrap().0, "digest:2026-W47");
        assert_eq!(digest_due(at(22, 23)).unwrap().0, "digest:2026-W47");
        assert_eq!(digest_due(at(23, 7)), None);
    }

    #[test]
    fn digest_keys_use_the_iso_year() {
        // 2027-01-04 is the Monday of ISO week 1 of 2027.
        let monday = NaiveDate::from_ymd_opt(2027, 1, 4)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap();
        assert_eq!(digest_due(monday).unwrap().0, "digest:2027-W01");
        // 2026-12-28 is the Monday of ISO week 53 of 2026.
        let late = NaiveDate::from_ymd_opt(2026, 12, 28)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap();
        assert_eq!(digest_due(late).unwrap().0, "digest:2026-W53");
    }
}
