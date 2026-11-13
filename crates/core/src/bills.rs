//! Bills: when they are due, which due date a payment belongs to, and when
//! to remind (ARCHITECTURE.md 4.3 and 8.3). Pure functions; "today" is a
//! parameter.

use chrono::{Days, Months, NaiveDate};
use serde::{Deserialize, Serialize};

/// How often a bill repeats (before `skip`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillFrequency {
    Weekly,
    Monthly,
    Quarterly,
    HalfYear,
    Yearly,
}

impl BillFrequency {
    pub fn as_str(self) -> &'static str {
        match self {
            BillFrequency::Weekly => "weekly",
            BillFrequency::Monthly => "monthly",
            BillFrequency::Quarterly => "quarterly",
            BillFrequency::HalfYear => "half_year",
            BillFrequency::Yearly => "yearly",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "weekly" => Some(BillFrequency::Weekly),
            "monthly" => Some(BillFrequency::Monthly),
            "quarterly" => Some(BillFrequency::Quarterly),
            "half_year" => Some(BillFrequency::HalfYear),
            "yearly" => Some(BillFrequency::Yearly),
            _ => None,
        }
    }

    /// Months per period, or `None` for weekly bills.
    fn months(self) -> Option<u32> {
        match self {
            BillFrequency::Weekly => None,
            BillFrequency::Monthly => Some(1),
            BillFrequency::Quarterly => Some(3),
            BillFrequency::HalfYear => Some(6),
            BillFrequency::Yearly => Some(12),
        }
    }
}

/// The due dates of a bill: the first one, then every `frequency`, with
/// `skip` periods left out in between (skip 1 on monthly = every 2 months).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BillSchedule {
    pub first_due_date: NaiveDate,
    pub frequency: BillFrequency,
    pub skip: u32,
    /// No due dates after this day.
    pub end_date: Option<NaiveDate>,
}

impl BillSchedule {
    /// The `n`-th due date (0 = the first), or `None` after the end date.
    pub fn nth(&self, n: u32) -> Option<NaiveDate> {
        self.unbounded_nth(n)
            .filter(|date| self.end_date.is_none_or(|end| *date <= end))
    }

    /// The `n`-th due date ignoring the end date. Each date is computed from
    /// the first one, never from the previous one, so a bill on the 31st
    /// comes back to the 31st after a short month (Carbon's `addMonthNoOverflow`
    /// from a fixed start, rather than chaining).
    fn unbounded_nth(&self, n: u32) -> Option<NaiveDate> {
        let periods = n.checked_mul(self.skip.checked_add(1)?)?;
        match self.frequency.months() {
            // `checked_add_months` clamps to the month's last day (31 → 30/28).
            Some(months) => self
                .first_due_date
                .checked_add_months(Months::new(periods.checked_mul(months)?)),
            None => self
                .first_due_date
                .checked_add_days(Days::new(u64::from(periods) * 7)),
        }
    }

    /// Every due date from `from` to `to`, both included.
    pub fn occurrences(&self, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
        // `(0..)` counts up forever; `map_while` stops at the first `None`
        // (past the end date or overflow) and `take_while` at the first date
        // past `to`. Iterators are lazy, like a PHP generator: nothing beyond
        // `to` is computed.
        (0..)
            .map_while(|n| self.nth(n))
            .take_while(|date| *date <= to)
            .filter(|date| *date >= from)
            .collect()
    }

    /// Which of `open` due dates a payment on `date` settles: the nearest one
    /// whose window (from the due date before it to the one after it)
    /// contains `date`. `None` if no open period is near enough.
    pub fn matching_due_date(&self, open: &[NaiveDate], date: NaiveDate) -> Option<NaiveDate> {
        open.iter()
            .copied()
            .filter(|due| {
                let (before, after) = self.neighbours(*due);
                before < date && date < after
            })
            // Nearest first; on a tie the earlier due date wins.
            .min_by_key(|due| ((*due - date).num_days().abs(), *due))
    }

    /// The due dates just before and just after `due`. Before the first one
    /// (and after the last) the same distance is assumed.
    fn neighbours(&self, due: NaiveDate) -> (NaiveDate, NaiveDate) {
        let index = (0..)
            .map_while(|n| self.unbounded_nth(n).map(|d| (n, d)))
            .find(|(_, d)| *d >= due)
            .map_or(0, |(n, _)| n);
        let after = self.unbounded_nth(index + 1).unwrap_or(due);
        let before = match index.checked_sub(1).and_then(|n| self.unbounded_nth(n)) {
            Some(before) => before,
            // No earlier due date: mirror the distance to the next one.
            None => due - (after - due),
        };
        (before, after)
    }
}

/// The parts of a bill that decide reminders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bill {
    pub schedule: BillSchedule,
    pub remind_days_before: u32,
    pub remind_every_days: u32,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeriodStatus {
    Open,
    Paid,
    Skipped,
}

/// One due date of a bill and its reminder state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BillPeriod {
    pub due_date: NaiveDate,
    pub status: PeriodStatus,
    pub last_reminded_on: Option<NaiveDate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReminderKind {
    DueSoon,
    Overdue,
}

impl ReminderKind {
    /// The event type the reminder is recorded as.
    pub fn event_type(self) -> &'static str {
        match self {
            ReminderKind::DueSoon => "due_soon",
            ReminderKind::Overdue => "overdue",
        }
    }
}

/// Whether to remind about `period` today (ARCHITECTURE.md 8.3): once
/// `remind_days_before` days before the due date, then after the due date
/// every `remind_every_days` days until it is paid or skipped.
pub fn reminder_due(period: &BillPeriod, bill: &Bill, today: NaiveDate) -> Option<ReminderKind> {
    if !bill.active || period.status != PeriodStatus::Open {
        return None;
    }
    if today > period.due_date {
        // `match` on an `Option` with a guard: like a PHP `match (true)`
        // whose arms also unpack the value.
        return match period.last_reminded_on {
            None => Some(ReminderKind::Overdue),
            Some(last) if (today - last).num_days() >= i64::from(bill.remind_every_days) => {
                Some(ReminderKind::Overdue)
            }
            Some(_) => None,
        };
    }
    let first_reminder = period.due_date - Days::new(u64::from(bill.remind_days_before));
    if today >= first_reminder && period.last_reminded_on.is_none() {
        return Some(ReminderKind::DueSoon);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(text: &str) -> NaiveDate {
        text.parse().unwrap()
    }

    fn schedule(first: &str, frequency: BillFrequency, skip: u32) -> BillSchedule {
        BillSchedule {
            first_due_date: d(first),
            frequency,
            skip,
            end_date: None,
        }
    }

    fn dates(list: &[NaiveDate]) -> Vec<String> {
        list.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn frequencies_round_trip_through_text() {
        for f in [
            BillFrequency::Weekly,
            BillFrequency::Monthly,
            BillFrequency::Quarterly,
            BillFrequency::HalfYear,
            BillFrequency::Yearly,
        ] {
            assert_eq!(BillFrequency::parse(f.as_str()), Some(f));
        }
        assert_eq!(BillFrequency::HalfYear.as_str(), "half_year");
        assert_eq!(BillFrequency::parse("fortnightly"), None);
    }

    #[test]
    fn month_end_is_clamped_without_drifting() {
        let s = schedule("2026-01-31", BillFrequency::Monthly, 0);
        let got = s.occurrences(d("2026-01-01"), d("2026-05-31"));
        assert_eq!(
            dates(&got),
            [
                "2026-01-31",
                "2026-02-28",
                "2026-03-31",
                "2026-04-30",
                "2026-05-31"
            ]
        );
    }

    #[test]
    fn leap_days_fall_back_to_february_28() {
        let s = schedule("2024-02-29", BillFrequency::Yearly, 0);
        let got = s.occurrences(d("2024-01-01"), d("2028-12-31"));
        assert_eq!(
            dates(&got),
            [
                "2024-02-29",
                "2025-02-28",
                "2026-02-28",
                "2027-02-28",
                "2028-02-29"
            ]
        );
    }

    #[test]
    fn every_frequency_and_skip() {
        let weekly = schedule("2026-11-02", BillFrequency::Weekly, 1);
        assert_eq!(
            dates(&weekly.occurrences(d("2026-11-01"), d("2026-11-30"))),
            ["2026-11-02", "2026-11-16", "2026-11-30"]
        );
        let quarterly = schedule("2026-01-15", BillFrequency::Quarterly, 0);
        assert_eq!(
            dates(&quarterly.occurrences(d("2026-01-01"), d("2026-12-31"))),
            ["2026-01-15", "2026-04-15", "2026-07-15", "2026-10-15"]
        );
        let half = schedule("2026-08-31", BillFrequency::HalfYear, 0);
        assert_eq!(
            dates(&half.occurrences(d("2026-01-01"), d("2027-12-31"))),
            ["2026-08-31", "2027-02-28", "2027-08-31"]
        );
        let every_other_month = schedule("2026-01-10", BillFrequency::Monthly, 1);
        assert_eq!(
            dates(&every_other_month.occurrences(d("2026-01-01"), d("2026-06-30"))),
            ["2026-01-10", "2026-03-10", "2026-05-10"]
        );
    }

    #[test]
    fn range_and_end_date_limit_the_occurrences() {
        let mut s = schedule("2026-01-05", BillFrequency::Monthly, 0);
        assert_eq!(
            dates(&s.occurrences(d("2026-03-01"), d("2026-04-30"))),
            ["2026-03-05", "2026-04-05"]
        );
        assert!(s.occurrences(d("2025-01-01"), d("2025-12-31")).is_empty());
        s.end_date = Some(d("2026-03-05"));
        assert_eq!(
            dates(&s.occurrences(d("2026-01-01"), d("2026-12-31"))),
            ["2026-01-05", "2026-02-05", "2026-03-05"]
        );
        assert_eq!(s.nth(3), None);
        assert_eq!(s.nth(0), Some(d("2026-01-05")));
    }

    #[test]
    fn payments_settle_the_nearest_open_period() {
        let s = schedule("2026-01-02", BillFrequency::Monthly, 0);
        let open = [d("2026-11-02"), d("2026-12-02")];
        // Paid a few days late, early, or on time: the nearest due date.
        assert_eq!(
            s.matching_due_date(&open, d("2026-11-05")),
            Some(d("2026-11-02"))
        );
        assert_eq!(
            s.matching_due_date(&open, d("2026-11-29")),
            Some(d("2026-12-02"))
        );
        assert_eq!(
            s.matching_due_date(&open, d("2026-12-02")),
            Some(d("2026-12-02"))
        );
        // November already paid: a payment near it still settles December
        // if inside December's window.
        assert_eq!(
            s.matching_due_date(&[d("2026-12-02")], d("2026-11-05")),
            Some(d("2026-12-02"))
        );
        // Far away from every open period: nothing.
        assert_eq!(s.matching_due_date(&open, d("2026-06-01")), None);
        assert_eq!(s.matching_due_date(&[], d("2026-11-02")), None);
        // Before the first due date, within one period of it.
        assert_eq!(
            s.matching_due_date(&[d("2026-01-02")], d("2025-12-20")),
            Some(d("2026-01-02"))
        );
    }

    fn bill() -> Bill {
        Bill {
            schedule: schedule("2026-01-10", BillFrequency::Monthly, 0),
            remind_days_before: 3,
            remind_every_days: 2,
            active: true,
        }
    }

    fn period(last: Option<&str>) -> BillPeriod {
        BillPeriod {
            due_date: d("2026-11-10"),
            status: PeriodStatus::Open,
            last_reminded_on: last.map(d),
        }
    }

    #[test]
    fn due_soon_once_before_the_due_date() {
        assert_eq!(reminder_due(&period(None), &bill(), d("2026-11-06")), None);
        assert_eq!(
            reminder_due(&period(None), &bill(), d("2026-11-07")),
            Some(ReminderKind::DueSoon)
        );
        assert_eq!(
            reminder_due(&period(None), &bill(), d("2026-11-10")),
            Some(ReminderKind::DueSoon)
        );
        assert_eq!(
            reminder_due(&period(Some("2026-11-07")), &bill(), d("2026-11-08")),
            None
        );
        assert_eq!(
            reminder_due(&period(Some("2026-11-07")), &bill(), d("2026-11-10")),
            None
        );
    }

    #[test]
    fn overdue_every_n_days_after_the_due_date() {
        assert_eq!(
            reminder_due(&period(Some("2026-11-07")), &bill(), d("2026-11-11")),
            Some(ReminderKind::Overdue)
        );
        assert_eq!(
            reminder_due(&period(Some("2026-11-11")), &bill(), d("2026-11-12")),
            None
        );
        assert_eq!(
            reminder_due(&period(Some("2026-11-11")), &bill(), d("2026-11-13")),
            Some(ReminderKind::Overdue)
        );
        // Never reminded and already late: overdue straight away.
        assert_eq!(
            reminder_due(&period(None), &bill(), d("2026-11-20")),
            Some(ReminderKind::Overdue)
        );
    }

    #[test]
    fn nothing_for_settled_periods_or_inactive_bills() {
        let paid = BillPeriod {
            status: PeriodStatus::Paid,
            ..period(None)
        };
        let skipped = BillPeriod {
            status: PeriodStatus::Skipped,
            ..period(None)
        };
        assert_eq!(reminder_due(&paid, &bill(), d("2026-11-20")), None);
        assert_eq!(reminder_due(&skipped, &bill(), d("2026-11-20")), None);
        let inactive = Bill {
            active: false,
            ..bill()
        };
        assert_eq!(
            reminder_due(&period(None), &inactive, d("2026-11-20")),
            None
        );
    }

    #[test]
    fn reminder_kinds_name_their_events() {
        assert_eq!(ReminderKind::DueSoon.event_type(), "due_soon");
        assert_eq!(ReminderKind::Overdue.event_type(), "overdue");
    }
}
