//! The bill scheduler task: reminders for due and overdue bills
//! (ARCHITECTURE.md 8.3).

use saldo_app::bills::{REMINDER_INTERVAL, run_reminders};

use super::Workers;
use crate::state::AppState;

/// Runs at startup, every 15 minutes, and after writes (so a new or changed
/// bill is reminded about right away). Wakes the notifier when it recorded
/// reminders, since those events are written outside any HTTP request; the
/// run that this wakes in turn records nothing, so there is no loop.
pub fn spawn_bill_scheduler(workers: &mut Workers, state: &AppState) {
    let app = state.app.clone();
    let signal = state.events.clone();
    workers.spawn(
        "bill-scheduler",
        REMINDER_INTERVAL,
        Some(&state.events),
        move || {
            // Each run gets its own clones: the future must own what it uses,
            // because it may outlive this closure call (like capturing by value
            // in a Go goroutine).
            let (app, signal) = (app.clone(), signal.clone());
            async move {
                let recorded = run_reminders(&app.identity, &app.bills)
                    .await
                    .map_err(|e| e.to_string())?;
                if recorded > 0 {
                    signal.notify();
                }
                Ok(())
            }
        },
    );
}
