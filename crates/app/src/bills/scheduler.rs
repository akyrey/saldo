//! The bill scheduler's work (ARCHITECTURE.md 8.3): one pass over every
//! household. The server runs it at startup and every 15 minutes.

use std::time::Duration;

use chrono::NaiveDate;
use tracing::error;

use super::BillService;
use crate::ctx::HouseholdCtx;
use crate::error::AppError;
use crate::identity::IdentityService;

/// How often the server runs [`run_reminders`].
pub const REMINDER_INTERVAL: Duration = Duration::from_secs(15 * 60);

impl BillService {
    /// One household: creates missing periods, then records a reminder for
    /// every open period that is due one today. Returns how many reminders
    /// were recorded; running it again the same day records none.
    pub async fn remind(&self, ctx: &HouseholdCtx, today: NaiveDate) -> Result<u32, AppError> {
        self.materialise_periods(ctx, today).await?;
        let mut recorded = 0;
        for open in self.open_periods(ctx).await? {
            let Some(kind) = Self::reminder_due(&open, today) else {
                continue;
            };
            if self
                .record_reminder(ctx, open.period_id, kind, today)
                .await?
            {
                recorded += 1;
            }
        }
        Ok(recorded)
    }
}

/// Every household, each with its own "today" (its timezone). A failing
/// household is logged and skipped so it cannot block the others; the error
/// of the last failure is returned after all were tried.
pub async fn run_reminders(
    identity: &IdentityService,
    bills: &BillService,
) -> Result<u32, AppError> {
    let mut recorded = 0;
    let mut failure = None;
    for ctx in identity.household_contexts().await? {
        let result = match identity.household_today(&ctx).await {
            Ok(today) => bills.remind(&ctx, today).await,
            Err(e) => Err(e),
        };
        match result {
            Ok(n) => recorded += n,
            Err(e) => {
                error!(household = %ctx.household_id, error = %e, "bill reminders failed");
                failure = Some(e);
            }
        }
    }
    match failure {
        Some(e) => Err(e),
        None => Ok(recorded),
    }
}
