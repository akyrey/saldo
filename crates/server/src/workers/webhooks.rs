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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use saldo_app::clock::SystemClock;
    use saldo_app::ctx::Origin;
    use saldo_app::identity::SetupInput;
    use saldo_app::webhooks::NewWebhook;
    use saldo_app::{App, db};
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn new_events_are_dispatched_and_attempted() {
        let pool = db::connect_in_memory().await.unwrap();
        db::migrate(&pool).await.unwrap();
        let app = App::new(pool.clone(), Arc::new(SystemClock));
        let setup = SetupInput {
            email: "o@example.com".into(),
            password: "correct horse battery".into(),
            display_name: "O".into(),
            household_name: "Home".into(),
            primary_currency: "EUR".into(),
        };
        let ctx = app.identity.setup(Origin::Api, setup).await.unwrap().ctx;
        let state = AppState::new(app.clone(), Config::from_lookup(|_| None).unwrap());
        let mut workers = Workers::new(CancellationToken::new());
        register(&mut workers, &state);
        assert_eq!(workers.names(), ["webhook-dispatcher", "webhook-delivery"]);

        // Port 9 (discard) is closed: the attempt fails and is recorded.
        let hook = NewWebhook {
            name: "Closed".into(),
            url: "http://127.0.0.1:9/hook".into(),
            ..NewWebhook::default()
        };
        app.webhooks.create(&ctx, Origin::Api, hook).await.unwrap();
        state.events.notify();

        let mut attempts = 0;
        for _ in 0..100 {
            (attempts,) = sqlx::query_as::<_, (i64,)>(
                "SELECT coalesce(max(attempts), 0) FROM webhook_deliveries",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            if attempts > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(attempts, 1);
        workers.shutdown().await;
    }
}
