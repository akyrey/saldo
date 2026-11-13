//! Recurring transactions: when a recurrence fires (ARCHITECTURE.md 4.3, 5.4).
//!
//! The types here mirror the `recurrence_repetitions` table: a pattern
//! (`kind` + `moment`), a `skip` count and a weekend policy. The dates
//! themselves are computed by [`occurrences`] in `schedule.rs`.

mod schedule;

use std::fmt;
use std::str::FromStr;

use chrono::{NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::error::IdError;

pub use schedule::occurrences;

/// Identifies a recurring transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecurrenceId(pub Uuid);

impl fmt::Display for RecurrenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for RecurrenceId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(Self).map_err(|_| IdError {
            input: s.to_string(),
        })
    }
}

/// Which days a repetition picks, before `skip` and the weekend policy.
///
/// An enum whose variants carry data: `Monthly(31)` holds the day, while
/// `Daily` holds nothing. A PHP enum cannot carry per-case values; in Go you
/// would write an interface with one struct per case and a type switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pattern {
    Daily,
    Weekly(Weekday),
    /// Day of the month, 1 to 31; clamped to the month's last day.
    Monthly(u32),
    /// The `week`-th `weekday` of the month (1 to 5). A fifth weekday that a
    /// month does not have becomes that month's last one.
    NthWeekday {
        week: u32,
        weekday: Weekday,
    },
    /// Month and day; 29 February becomes 28 February outside leap years.
    Yearly {
        month: u32,
        day: u32,
    },
}

/// What happens to a date that falls on a Saturday or Sunday.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WeekendPolicy {
    #[default]
    Keep,
    /// Drop the date.
    Skip,
    PreviousFriday,
    NextMonday,
}

/// One row of `recurrence_repetitions`. `skip = 1` means every second slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repetition {
    pub pattern: Pattern,
    pub skip: u32,
    pub weekend: WeekendPolicy,
}

/// Everything that decides the dates of a recurrence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecurrenceSchedule {
    pub first_date: NaiveDate,
    /// Last allowed date (inclusive).
    pub repeat_until: Option<NaiveDate>,
    /// Stop after this many dates, counted from `first_date`.
    pub max_occurrences: Option<u32>,
    pub repetitions: Vec<Repetition>,
}

/// A stored or submitted repetition that does not make sense.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScheduleError {
    #[error("unknown repetition kind {0:?} (daily, weekly, monthly, nth_weekday, yearly)")]
    UnknownKind(String),
    #[error("invalid moment {moment:?} for {kind}: {expected}")]
    InvalidMoment {
        kind: &'static str,
        moment: String,
        expected: &'static str,
    },
    #[error("unknown weekend policy {0:?} (keep, skip, previous_friday, next_monday)")]
    UnknownWeekendPolicy(String),
}

impl Pattern {
    /// Builds a pattern from the stored `kind` and `moment` columns:
    /// `''` for daily, `1`-`7` (Monday to Sunday) for weekly, `1`-`31` for
    /// monthly, `week,weekday` for nth_weekday and `MM-DD` for yearly.
    pub fn parse(kind: &str, moment: &str) -> Result<Pattern, ScheduleError> {
        let moment = moment.trim();
        match kind {
            "daily" => Ok(Pattern::Daily),
            "weekly" => weekday(moment)
                .map(Pattern::Weekly)
                .ok_or_else(|| invalid("weekly", moment, "a weekday from 1 (Monday) to 7")),
            "monthly" => number(moment, 1, 31)
                .map(Pattern::Monthly)
                .ok_or_else(|| invalid("monthly", moment, "a day from 1 to 31")),
            "nth_weekday" => parse_nth(moment).ok_or_else(|| {
                invalid("nth_weekday", moment, "week,weekday such as 2,3 (1-5, 1-7)")
            }),
            "yearly" => parse_yearly(moment)
                .ok_or_else(|| invalid("yearly", moment, "a month and day such as 03-15")),
            other => Err(ScheduleError::UnknownKind(other.to_string())),
        }
    }

    /// The stored `kind` column.
    pub fn kind(&self) -> &'static str {
        match self {
            Pattern::Daily => "daily",
            Pattern::Weekly(_) => "weekly",
            Pattern::Monthly(_) => "monthly",
            Pattern::NthWeekday { .. } => "nth_weekday",
            Pattern::Yearly { .. } => "yearly",
        }
    }

    /// The stored `moment` column; `parse(kind(), moment())` gives `self` back.
    pub fn moment(&self) -> String {
        match self {
            Pattern::Daily => String::new(),
            Pattern::Weekly(day) => day.number_from_monday().to_string(),
            Pattern::Monthly(day) => day.to_string(),
            Pattern::NthWeekday { week, weekday } => {
                format!("{week},{}", weekday.number_from_monday())
            }
            Pattern::Yearly { month, day } => format!("{month:02}-{day:02}"),
        }
    }
}

impl WeekendPolicy {
    pub fn parse(text: &str) -> Result<WeekendPolicy, ScheduleError> {
        match text {
            "keep" => Ok(WeekendPolicy::Keep),
            "skip" => Ok(WeekendPolicy::Skip),
            "previous_friday" => Ok(WeekendPolicy::PreviousFriday),
            "next_monday" => Ok(WeekendPolicy::NextMonday),
            other => Err(ScheduleError::UnknownWeekendPolicy(other.to_string())),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            WeekendPolicy::Keep => "keep",
            WeekendPolicy::Skip => "skip",
            WeekendPolicy::PreviousFriday => "previous_friday",
            WeekendPolicy::NextMonday => "next_monday",
        }
    }
}

fn invalid(kind: &'static str, moment: &str, expected: &'static str) -> ScheduleError {
    ScheduleError::InvalidMoment {
        kind,
        moment: moment.to_string(),
        expected,
    }
}

/// A whole number in `min..=max`; no signs, spaces or empty text.
fn number(text: &str, min: u32, max: u32) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok().filter(|n| (min..=max).contains(n))
}

/// `1` is Monday, `7` is Sunday.
fn weekday(text: &str) -> Option<Weekday> {
    let n = number(text, 1, 7)?;
    // `Weekday::try_from(u8)` counts from Monday = 0.
    Weekday::try_from(u8::try_from(n - 1).ok()?).ok()
}

fn parse_nth(text: &str) -> Option<Pattern> {
    let (week, day) = text.split_once(',')?;
    Some(Pattern::NthWeekday {
        week: number(week.trim(), 1, 5)?,
        weekday: weekday(day.trim())?,
    })
}

fn parse_yearly(text: &str) -> Option<Pattern> {
    let (month, day) = text.split_once('-')?;
    let (month, day) = (number(month, 1, 12)?, number(day, 1, 31)?);
    // 2028 is a leap year, so 02-29 is accepted; 02-30 and 04-31 are not.
    NaiveDate::from_ymd_opt(2028, month, day)?;
    Some(Pattern::Yearly { month, day })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_round_trip_through_kind_and_moment() {
        let patterns = [
            Pattern::Daily,
            Pattern::Weekly(Weekday::Mon),
            Pattern::Weekly(Weekday::Sun),
            Pattern::Monthly(31),
            Pattern::NthWeekday {
                week: 2,
                weekday: Weekday::Wed,
            },
            Pattern::Yearly { month: 2, day: 29 },
        ];
        for pattern in patterns {
            assert_eq!(
                Pattern::parse(pattern.kind(), &pattern.moment()).unwrap(),
                pattern
            );
        }
        assert_eq!(Pattern::Yearly { month: 3, day: 5 }.moment(), "03-05");
        assert_eq!(Pattern::Weekly(Weekday::Sun).moment(), "7");
    }

    #[test]
    fn bad_moments_are_rejected_with_the_kind() {
        for (kind, moment) in [
            ("weekly", "0"),
            ("weekly", "8"),
            ("weekly", ""),
            ("monthly", "32"),
            ("monthly", "0"),
            ("monthly", "-1"),
            ("monthly", "+3"),
            ("nth_weekday", "6,1"),
            ("nth_weekday", "2"),
            ("nth_weekday", "2,8"),
            ("yearly", "02-30"),
            ("yearly", "13-01"),
            ("yearly", "2026-03-15"),
        ] {
            let error = Pattern::parse(kind, moment).unwrap_err();
            assert!(
                matches!(error, ScheduleError::InvalidMoment { .. }),
                "{kind} {moment}"
            );
        }
        assert_eq!(
            Pattern::parse("hourly", "").unwrap_err(),
            ScheduleError::UnknownKind("hourly".to_string())
        );
        assert!(
            Pattern::parse("monthly", "32")
                .unwrap_err()
                .to_string()
                .contains("monthly")
        );
    }

    #[test]
    fn daily_ignores_the_moment_and_spaces_are_trimmed() {
        assert_eq!(Pattern::parse("daily", "x").unwrap(), Pattern::Daily);
        assert_eq!(
            Pattern::parse("nth_weekday", " 2, 3 ").unwrap(),
            Pattern::NthWeekday {
                week: 2,
                weekday: Weekday::Wed
            }
        );
    }

    #[test]
    fn weekend_policies_round_trip() {
        for policy in [
            WeekendPolicy::Keep,
            WeekendPolicy::Skip,
            WeekendPolicy::PreviousFriday,
            WeekendPolicy::NextMonday,
        ] {
            assert_eq!(WeekendPolicy::parse(policy.as_str()).unwrap(), policy);
        }
        assert!(WeekendPolicy::parse("sunday").is_err());
        assert_eq!(WeekendPolicy::default(), WeekendPolicy::Keep);
    }

    #[test]
    fn recurrence_ids_parse_and_display() {
        let text = "0190f5c3-7a2b-7c00-8000-000000000001";
        let id: RecurrenceId = text.parse().unwrap();
        assert_eq!(id.to_string(), text);
        assert!("nope".parse::<RecurrenceId>().is_err());
    }
}
