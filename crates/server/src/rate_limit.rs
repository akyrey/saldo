//! A small in-memory limiter for login attempts, per IP and per email.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Attempts allowed per key within [`WINDOW`].
pub const MAX_ATTEMPTS: usize = 10;
pub const WINDOW: Duration = Duration::from_secs(15 * 60);

/// Remembers recent attempts per key (`ip:...`, `email:...`). State lives in
/// memory, so it resets on restart; that is enough for a household server.
#[derive(Debug)]
pub struct LoginLimiter {
    attempts: Mutex<HashMap<String, Vec<Instant>>>,
    max_attempts: usize,
    window: Duration,
}

impl Default for LoginLimiter {
    fn default() -> Self {
        Self::new(MAX_ATTEMPTS, WINDOW)
    }
}

impl LoginLimiter {
    pub fn new(max_attempts: usize, window: Duration) -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
            max_attempts,
            window,
        }
    }

    /// Records an attempt for every key, unless one of them is already at the
    /// limit; returns whether the attempt may go ahead.
    pub fn allow(&self, keys: &[String], now: Instant) -> bool {
        let mut attempts = self
            .attempts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Forget attempts that fell out of the window, and empty keys.
        attempts.retain(|_, times| {
            times.retain(|t| now.duration_since(*t) < self.window);
            !times.is_empty()
        });
        let blocked = keys.iter().any(|key| {
            attempts
                .get(key)
                .is_some_and(|t| t.len() >= self.max_attempts)
        });
        if blocked {
            return false;
        }
        for key in keys {
            attempts.entry(key.clone()).or_default().push(now);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(email: &str) -> Vec<String> {
        vec!["ip:1.2.3.4".to_string(), format!("email:{email}")]
    }

    #[test]
    fn blocks_after_the_limit_until_the_window_passes() {
        let limiter = LoginLimiter::new(3, Duration::from_secs(60));
        let start = Instant::now();
        for _ in 0..3 {
            assert!(limiter.allow(&keys("a"), start));
        }
        assert!(!limiter.allow(&keys("a"), start));
        assert!(limiter.allow(&keys("a"), start + Duration::from_secs(61)));
    }

    #[test]
    fn any_exhausted_key_blocks() {
        let limiter = LoginLimiter::new(2, Duration::from_secs(60));
        let now = Instant::now();
        assert!(limiter.allow(&keys("a"), now));
        assert!(limiter.allow(&keys("b"), now));
        // Same IP, third email: the IP key is exhausted.
        assert!(!limiter.allow(&keys("c"), now));
        // A different IP for an email with one attempt left still works.
        assert!(limiter.allow(&["ip:5.6.7.8".to_string(), "email:a".to_string()], now));
    }
}
