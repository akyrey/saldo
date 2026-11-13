//! Webhooks (ARCHITECTURE.md 8.2): endpoints the owner registers, a
//! dispatcher that turns events into deliveries, and a delivery worker that
//! POSTs them, signed, with retries.
//!
//! Endpoint changes append `webhook.*` events like every other entity.
//! Delivery bookkeeping (attempts, status, redeliver) appends none: it is the
//! outbox itself, and an event per attempt would feed back into webhooks.

mod delivery;
mod dispatch;
mod endpoints;
pub mod matching;
mod repo;
pub mod retry;
pub mod signing;

use std::time::Duration;

use serde::Serialize;
use sqlx::SqlitePool;

use crate::clock::SharedClock;

pub use delivery::DeliveryReport;
pub use dispatch::DispatchReport;
pub use repo::DeliveryView;

/// How long a receiver may take to answer.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// An endpoint as the API returns it. Never contains the secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct WebhookView {
    pub id: String,
    pub name: String,
    pub url: String,
    /// Exact types (`transaction.created`), prefixes (`transaction.*`) or `*`.
    pub event_types: Vec<String>,
    /// Also send events written by the Firefly importer.
    pub include_import_events: bool,
    pub active: bool,
    /// Dead deliveries in a row; 20 deactivate the endpoint.
    pub consecutive_failures: i64,
    /// Why the server deactivated the endpoint, if it did.
    pub disabled_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// An endpoint together with its signing secret: returned once, by create
/// and rotate-secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct WebhookWithSecret {
    pub webhook: WebhookView,
    /// The HMAC key. Store it now: it is never shown again.
    pub secret: String,
}

/// Input for a new endpoint.
#[derive(Debug, Clone, Default)]
pub struct NewWebhook {
    pub name: String,
    pub url: String,
    /// `None` means every event (`["*"]`).
    pub event_types: Option<Vec<String>>,
    pub include_import_events: bool,
}

/// `None` keeps a value. Setting `active` to true also clears the failure
/// counter and the disabled reason.
#[derive(Debug, Clone, Default)]
pub struct WebhookUpdate {
    pub name: Option<String>,
    pub url: Option<String>,
    pub event_types: Option<Vec<String>>,
    pub include_import_events: Option<bool>,
    pub active: Option<bool>,
}

/// Endpoint management, dispatching and delivery.
#[derive(Clone)]
pub struct WebhookService {
    pool: SqlitePool,
    clock: SharedClock,
    timeout: Duration,
}

impl WebhookService {
    pub fn new(pool: SqlitePool, clock: SharedClock) -> Self {
        Self {
            pool,
            clock,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// The same service with another receiver timeout (tests use a short one).
    pub fn with_timeout(self, timeout: Duration) -> Self {
        Self { timeout, ..self }
    }
}

/// A new secret: `whsec_` and 64 hex characters (256 random bits).
fn new_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("whsec_{hex}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_random_and_prefixed() {
        let (a, b) = (new_secret(), new_secret());
        assert!(a.starts_with("whsec_"));
        assert_eq!(a.len(), 6 + 64);
        assert_ne!(a, b);
    }
}
