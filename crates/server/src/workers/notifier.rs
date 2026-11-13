//! The notifier task (ARCHITECTURE.md 8.4): turns reminder events into
//! notifications and sends them, retrying failures on later runs.

use std::time::Duration;

use saldo_app::App;
use saldo_app::error::AppError;
use tracing::warn;

use super::Workers;
use crate::notify::{NotifyError, Senders};
use crate::state::AppState;

/// Runs this often, and also whenever new events were written.
pub const NOTIFIER_INTERVAL: Duration = Duration::from_secs(60);
/// Events read, and notifications sent, per step.
const BATCH: i64 = 200;

pub fn spawn_notifier(workers: &mut Workers, state: &AppState) {
    let app = state.app.clone();
    let senders = state.senders.clone();
    workers.spawn(
        "notifier",
        NOTIFIER_INTERVAL,
        Some(&state.events),
        move || {
            let (app, senders) = (app.clone(), senders.clone());
            async move {
                run_notifier(&app, &senders)
                    .await
                    .map_err(|e| e.to_string())
            }
        },
    );
}

/// One run: catch up with the event log, then try every pending
/// notification once. A failing channel only records its own failure, so it
/// never holds up the others; after four attempts it is `failed`.
/// Restart-safe: progress is in the database (a crash between a send and its
/// bookkeeping can repeat that one send).
pub async fn run_notifier(app: &App, senders: &Senders) -> Result<(), AppError> {
    let batch = usize::try_from(BATCH).unwrap_or(usize::MAX);
    while app.notifications.fan_out(BATCH).await? == batch {}
    for pending in app.notifications.pending(BATCH).await? {
        let result = match &pending.message {
            Some(message) => senders.send(&pending.channel, message).await,
            None => Err(NotifyError::Failed(
                "the event cannot be turned into a message".to_string(),
            )),
        };
        match result {
            Ok(()) => app.notifications.mark_sent(&pending.id).await?,
            Err(error) => {
                // The error text never holds secrets (see `NotifyError`).
                warn!(notification = %pending.id, channel = %pending.channel.id, %error, "notification send failed");
                app.notifications
                    .mark_attempt_failed(&pending.id, &error.to_string())
                    .await?;
            }
        }
    }
    Ok(())
}
