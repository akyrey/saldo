//! Time and ID generation, behind a trait so tests can control them.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use uuid::{NoContext, Timestamp, Uuid};

/// Where the current time comes from.
///
/// A trait is Rust's interface. Services hold an `Arc<dyn Clock>` and never
/// call `Utc::now()` themselves, so tests can swap in a [`FixedClock`]: a Go
/// interface with a fake implementation, or Laravel's `Carbon::setTestNow`
/// without the global state. `Send + Sync` promise the clock may be shared
/// between threads; the compiler checks it, where Go relies on the race detector.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// The real clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// A clock that only moves when told to.
#[derive(Debug)]
pub struct FixedClock {
    // A `Mutex` lets `advance` change the time through a shared reference,
    // like a Go struct with a `sync.Mutex` field.
    now: Mutex<DateTime<Utc>>,
}

impl FixedClock {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            now: Mutex::new(now),
        }
    }

    /// Moves the clock forward.
    pub fn advance(&self, by: TimeDelta) {
        // A poisoned mutex only means another thread panicked while holding
        // it; the time value is still usable.
        let mut now = self
            .now
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *now += by;
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        *self
            .now
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Formats a timestamp the way the database stores it:
/// `2026-11-11T20:21:00.000Z`, so text order is time order.
pub fn format_timestamp(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// A new UUIDv7 whose time part comes from `clock`.
pub fn new_id(clock: &dyn Clock) -> Uuid {
    let now = clock.now();
    let seconds = u64::try_from(now.timestamp()).unwrap_or(0);
    Uuid::new_v7(Timestamp::from_unix(
        NoContext,
        seconds,
        now.timestamp_subsec_nanos(),
    ))
}

/// What every service needs besides the database: the clock.
///
/// `Arc` is a reference-counted pointer: cloning it is cheap and every clone
/// points at the same clock. Go would share a plain pointer and let the GC
/// clean up; Rust counts the owners and frees the value when the last goes.
pub type SharedClock = Arc<dyn Clock>;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed() -> FixedClock {
        FixedClock::new(Utc.with_ymd_and_hms(2026, 11, 11, 20, 21, 0).unwrap())
    }

    #[test]
    fn fixed_clock_moves_only_when_advanced() {
        let clock = fixed();
        let before = clock.now();
        assert_eq!(clock.now(), before);
        clock.advance(TimeDelta::minutes(5));
        assert_eq!(clock.now() - before, TimeDelta::minutes(5));
    }

    #[test]
    fn timestamps_have_milliseconds_and_z() {
        assert_eq!(format_timestamp(fixed().now()), "2026-11-11T20:21:00.000Z");
    }

    #[test]
    fn ids_are_v7_and_carry_the_clock_time() {
        let clock = fixed();
        let id = new_id(&clock);
        assert_eq!(id.get_version_num(), 7);
        let (seconds, _) = id.get_timestamp().unwrap().to_unix();
        assert_eq!(seconds, 1_794_428_460);
        assert_ne!(new_id(&clock), id);
    }

    #[test]
    fn system_clock_is_close_to_now() {
        let delta = SystemClock.now() - Utc::now();
        assert!(delta.num_seconds().abs() < 5);
    }
}
