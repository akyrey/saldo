//! The recurrence runner: fires every recurring transaction that is due up
//! to each household's today, at startup and then hourly.

use std::time::Duration;

use tracing::{info, warn};

use super::Workers;
use crate::state::AppState;

pub const NAME: &str = "recurrence-runner";
pub const INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Registers the runner. Not woken by new events: firing depends on the
/// date, not on what other requests write.
pub fn spawn(workers: &mut Workers, state: &AppState) {
    let state = state.clone();
    workers.spawn(NAME, INTERVAL, None, move || {
        // Each run gets its own clone, so the future owns everything it uses
        // (`async move`), like a Go goroutine capturing copies of its inputs.
        let state = state.clone();
        async move { run_once(&state).await.map(|_| ()) }
    });
}

/// One pass over every household. Returns how many transactions were
/// created; a date that cannot fire is logged and retried next run.
pub async fn run_once(state: &AppState) -> Result<usize, String> {
    let contexts = state
        .app
        .identity
        .household_contexts()
        .await
        .map_err(|e| e.to_string())?;
    let mut fired = 0;
    for ctx in contexts {
        let report = state
            .app
            .recurrences
            .fire_due(&ctx)
            .await
            .map_err(|e| e.to_string())?;
        for failure in &report.failures {
            warn!(
                household = %ctx.household_id,
                recurrence = %failure.recurrence_id,
                date = %failure.date,
                error = %failure.message,
                "recurrence could not fire"
            );
        }
        fired += report.fired.len();
    }
    if fired > 0 {
        info!(count = fired, "recurring transactions created");
        state.events.notify();
    }
    Ok(fired)
}
