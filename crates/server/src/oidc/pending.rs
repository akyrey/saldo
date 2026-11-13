//! Logins on their way through the provider, keyed by the `state` value.
//! Kept in memory: a restart only means clicking the login button again.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A login that is not back from the provider after this long is dropped.
const MAX_AGE: Duration = Duration::from_secs(600);

/// What the callback needs to finish one login.
#[derive(Debug, Clone)]
pub struct Pending {
    pub pkce_verifier: String,
    pub nonce: String,
    /// Where in the app to land afterwards (a local path).
    pub redirect: String,
    started: Instant,
}

impl Pending {
    pub fn new(pkce_verifier: String, nonce: String, redirect: String) -> Self {
        Self {
            pkce_verifier,
            nonce,
            redirect,
            started: Instant::now(),
        }
    }
}

/// `Mutex`: one lock around the map, like Go's `sync.Mutex`, except the
/// data lives inside it, so it cannot be touched without locking.
#[derive(Debug, Default)]
pub struct PendingLogins(Mutex<HashMap<String, Pending>>);

impl PendingLogins {
    pub fn insert(&self, state: String, pending: Pending) {
        let mut map = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        map.retain(|_, p| p.started.elapsed() < MAX_AGE);
        map.insert(state, pending);
    }

    /// Takes the login for `state` out (a state works once), if not expired.
    pub fn take(&self, state: &str) -> Option<Pending> {
        let mut map = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        map.remove(state).filter(|p| p.started.elapsed() < MAX_AGE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_works_once() {
        let logins = PendingLogins::default();
        logins.insert("s".into(), Pending::new("v".into(), "n".into(), "/".into()));
        assert!(logins.take("s").is_some());
        assert!(logins.take("s").is_none());
        assert!(logins.take("other").is_none());
    }
}
