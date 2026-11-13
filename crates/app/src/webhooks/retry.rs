//! The retry schedule (ARCHITECTURE.md 8.2): after a failed attempt, wait
//! 1 min, 5 min, 30 min, 2 h, 6 h, 12 h, 24 h, plus up to 10% jitter; after
//! the 8th failed attempt the delivery is dead.

use chrono::TimeDelta;

/// Attempts before a delivery is `dead`.
pub const MAX_ATTEMPTS: i64 = 8;

/// Consecutive dead deliveries that deactivate an endpoint.
pub const MAX_CONSECUTIVE_DEAD: i64 = 20;

/// Waits after attempt 1, 2, ... 7, in seconds.
const DELAYS_SECONDS: [i64; 7] = [
    60,
    5 * 60,
    30 * 60,
    2 * 3600,
    6 * 3600,
    12 * 3600,
    24 * 3600,
];

/// Jitter is at most delay / JITTER_DIVISOR (10%).
const JITTER_DIVISOR: i64 = 10;

/// How long to wait after `attempts` failed attempts, or `None` when the
/// delivery is dead. `random` picks the jitter (any value; it is reduced to
/// 0..=10% of the delay), so tests can pass a fixed number.
pub fn next_delay(attempts: i64, random: u64) -> Option<TimeDelta> {
    if attempts >= MAX_ATTEMPTS {
        return None;
    }
    let index = usize::try_from(attempts.max(1) - 1).ok()?;
    let base = *DELAYS_SECONDS.get(index)?;
    let max_jitter = base / JITTER_DIVISOR;
    // `max_jitter + 1` is never zero, and fits in u64 because it is positive.
    let jitter = random % u64::try_from(max_jitter + 1).unwrap_or(1);
    let jitter = i64::try_from(jitter).unwrap_or(0);
    Some(TimeDelta::seconds(base + jitter))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_the_schedule_without_jitter() {
        let minutes: Vec<i64> = (1..MAX_ATTEMPTS)
            .map(|n| next_delay(n, 0).unwrap().num_minutes())
            .collect();
        assert_eq!(minutes, [1, 5, 30, 120, 360, 720, 1440]);
    }

    #[test]
    fn jitter_adds_at_most_ten_percent() {
        assert_eq!(next_delay(1, 6).unwrap().num_seconds(), 66);
        assert_eq!(next_delay(1, 7).unwrap().num_seconds(), 60);
        for random in [1, 999, u64::MAX] {
            let delay = next_delay(7, random).unwrap().num_seconds();
            assert!((86_400..=95_040).contains(&delay), "{delay}");
        }
    }

    #[test]
    fn the_eighth_failure_is_final() {
        assert!(next_delay(MAX_ATTEMPTS, 0).is_none());
        assert!(next_delay(MAX_ATTEMPTS + 3, 0).is_none());
    }
}
