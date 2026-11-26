//! The credit card settlement sync (Phase 12): keeps each card month's
//! payment transfer equal to what the month spent until its payment day.

use std::time::Duration;

use saldo_app::App;
use tracing::{info, warn};

use super::Workers;
use crate::state::AppState;

pub const NAME: &str = "card-settlements";
pub const INTERVAL: Duration = Duration::from_secs(15 * 60);

/// Runs at startup, every 15 minutes (a payment day freezes its month) and
/// after writes (a purchase changes the transfer). Its own writes wake it
/// once more; that run changes nothing, so there is no loop.
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

/// One run over every household. A card that cannot be synced is logged
/// (without amounts or names) and tried again next run. Returns how many
/// changes were made.
pub async fn run(app: &App) -> Result<usize, String> {
    let households = app
        .identity
        .household_contexts()
        .await
        .map_err(|e| e.to_string())?;
    let mut changes = 0;
    for ctx in households {
        let report = app
            .credit_cards
            .sync_settlements(&ctx)
            .await
            .map_err(|e| e.to_string())?;
        for failure in &report.failures {
            warn!(
                household = %ctx.household_id,
                account = %failure.account_id,
                error = %failure.message,
                "credit card settlement could not be synced"
            );
        }
        changes += report.changes;
    }
    if changes > 0 {
        info!(count = changes, "credit card settlements changed");
    }
    Ok(changes)
}
