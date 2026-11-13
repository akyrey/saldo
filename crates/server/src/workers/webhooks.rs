//! The webhook dispatcher and delivery worker (ARCHITECTURE.md 8.2).

use std::time::Duration;

use super::Workers;
use crate::state::AppState;

/// Fallback for events written outside HTTP requests (the importer CLI).
const DISPATCH_INTERVAL: Duration = Duration::from_secs(60);
/// How often to look for retries that have become due.
const DELIVERY_INTERVAL: Duration = Duration::from_secs(15);

/// Starts both tasks. Both wake on new events; the dispatcher wakes the
/// delivery worker again once it has queued deliveries.
pub fn register(workers: &mut Workers, state: &AppState) {
    let (app, signal) = (state.app.clone(), state.events.clone());
    workers.spawn(
        "webhook-dispatcher",
        DISPATCH_INTERVAL,
        Some(&state.events),
        move || {
            // Each run gets its own clones: the closure is called again and
            // again, and the `async move` block takes ownership of what it
            // uses (like copying values into a goroutine's closure).
            let (app, signal) = (app.clone(), signal.clone());
            async move {
                let report = app.webhooks.dispatch().await.map_err(|e| e.to_string())?;
                if report.deliveries > 0 {
                    signal.notify();
                }
                Ok(())
            }
        },
    );

    let (app, signal) = (state.app.clone(), state.events.clone());
    workers.spawn(
        "webhook-delivery",
        DELIVERY_INTERVAL,
        Some(&state.events),
        move || {
            let (app, signal) = (app.clone(), signal.clone());
            async move {
                let report = app
                    .webhooks
                    .deliver_due()
                    .await
                    .map_err(|e| e.to_string())?;
                // A dead delivery may have deactivated an endpoint, which
                // appends a `webhook.updated` event for live listeners.
                if report.dead > 0 {
                    signal.notify();
                }
                Ok(())
            }
        },
    );
}
