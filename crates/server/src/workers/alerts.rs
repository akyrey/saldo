//! The alert scheduler (ARCHITECTURE.md 8.6): budget thresholds, the
//! forecast going below zero, goals reached and the weekly digest.

use std::time::Duration;

use saldo_app::App;

use super::Workers;
use crate::state::AppState;

pub const NAME: &str = "alert-scheduler";
pub const INTERVAL: Duration = Duration::from_secs(15 * 60);

/// Runs at startup, every 15 minutes and after writes (a transaction can
/// push a budget past a threshold). Wakes the notifier when it appended
/// alerts; the run that wake triggers appends nothing new, so there is no loop.
pub fn spawn(workers: &mut Workers, state: &AppState) {
    let app = state.app.clone();
    let signal = state.events.clone();
    workers.spawn(NAME, INTERVAL, Some(&state.events), move || {
        let (app, signal) = (app.clone(), signal.clone());
        async move {
            if run(&app).await? > 0 {
                signal.notify();
            }
            Ok(())
        }
    });
}

/// One run over every household. A failing household is reported and the
/// others still run. Returns how many alerts were appended.
pub async fn run(app: &App) -> Result<usize, String> {
    let households = app
        .identity
        .household_contexts()
        .await
        .map_err(|e| e.to_string())?;
    let mut appended = 0;
    let mut failures = Vec::new();
    for ctx in households {
        match app.alerts.run(&ctx).await {
            Ok(count) => appended += count,
            Err(error) => failures.push(format!("household {}: {error}", ctx.household_id)),
        }
    }
    if failures.is_empty() {
        Ok(appended)
    } else {
        Err(failures.join("; "))
    }
}
