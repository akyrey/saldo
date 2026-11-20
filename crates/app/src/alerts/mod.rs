//! Alerts (Phase 10, ARCHITECTURE.md 8.6): budget thresholds, the forecast
//! going below zero, net worth goals reached, and the weekly digest.
//!
//! A scheduler task calls [`AlertService::run`] for every household. Each
//! alert condition has a key; `alert_states` holds the keys that hold now.
//! A key that starts holding appends its event (once), a key that stops is
//! forgotten, so the next crossing alerts again.

mod budgets;
mod digest;
mod forecast;

use std::collections::BTreeSet;

use saldo_core::alerts::{diff, digest_due};
use serde_json::Value;
use sqlx::{SqliteConnection, SqlitePool};

use crate::budgets::BudgetService;
use crate::clock::{SharedClock, format_timestamp};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::AppError;
use crate::events::{NewEvent, append_event};
use crate::identity::now_in;
use crate::reports::ReportService;

/// How far ahead the forecast alert looks.
pub const FORECAST_ALERT_DAYS: u64 = 30;

/// One alert that holds now: its key and the event it appends when it starts.
#[derive(Debug, Clone)]
pub(crate) struct Condition {
    pub key: String,
    pub entity_type: &'static str,
    pub action: &'static str,
    pub entity_id: String,
    /// The event's `data`; [`AlertService::run`] adds `alert_key`.
    pub data: Value,
}

#[derive(Clone)]
pub struct AlertService {
    pool: SqlitePool,
    clock: SharedClock,
    budgets: BudgetService,
    reports: ReportService,
}

impl AlertService {
    pub fn new(
        pool: SqlitePool,
        clock: SharedClock,
        budgets: BudgetService,
        reports: ReportService,
    ) -> Self {
        Self {
            pool,
            clock,
            budgets,
            reports,
        }
    }

    /// Checks every condition of the household and appends the events of
    /// those that started, and the weekly digest when it is due. Returns how
    /// many events it appended. Safe to run again at any time.
    pub async fn run(&self, ctx: &HouseholdCtx) -> Result<usize, AppError> {
        let now = {
            let mut conn = self.pool.acquire().await?;
            now_in(&mut conn, ctx, &*self.clock).await?
        };
        let today = now.date();
        let (mut conditions, over) =
            budgets::conditions(&self.pool, &self.budgets, ctx, today).await?;
        conditions.extend(forecast::conditions(&self.pool, &self.reports, ctx, today).await?);
        let digest = match digest_due(now) {
            Some((key, week)) if !self.has_state(ctx, &key).await? => Some(
                digest::condition(&self.pool, &self.reports, ctx, key, week, today, &over).await?,
            ),
            _ => None,
        };

        let mut tx = crate::db::begin(&self.pool).await?;
        let mut appended = self.apply(&mut tx, ctx, &conditions).await?;
        if let Some(digest) = digest {
            // Only this week's key matters; older ones are dropped.
            let household_id = ctx.household_id.to_string();
            sqlx::query!(
                "DELETE FROM alert_states WHERE household_id = ? AND key LIKE 'digest:%' AND key != ?",
                household_id,
                digest.key
            )
            .execute(&mut *tx)
            .await?;
            appended += self.start(&mut tx, ctx, &digest).await?;
        }
        tx.commit().await?;
        Ok(appended)
    }

    /// Diffs the conditions against `alert_states`: inserts and alerts the
    /// new ones, forgets those that stopped.
    async fn apply(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        conditions: &[Condition],
    ) -> Result<usize, AppError> {
        let household_id = ctx.household_id.to_string();
        let held: BTreeSet<String> = sqlx::query_scalar!(
            "SELECT key FROM alert_states WHERE household_id = ? AND key NOT LIKE 'digest:%'",
            household_id
        )
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .collect();
        let now: BTreeSet<String> = conditions.iter().map(|c| c.key.clone()).collect();
        let (started, stopped) = diff(&held, &now);
        for key in stopped {
            sqlx::query!(
                "DELETE FROM alert_states WHERE household_id = ? AND key = ?",
                household_id,
                key
            )
            .execute(&mut *conn)
            .await?;
        }
        let mut appended = 0;
        for condition in conditions.iter().filter(|c| started.contains(&c.key)) {
            appended += self.start(&mut *conn, ctx, condition).await?;
        }
        Ok(appended)
    }

    /// Records the condition and appends its event; nothing when it was
    /// already recorded (a run that raced another).
    async fn start(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        condition: &Condition,
    ) -> Result<usize, AppError> {
        let household_id = ctx.household_id.to_string();
        let since = format_timestamp(self.clock.now());
        let inserted = sqlx::query!(
            "INSERT INTO alert_states (household_id, key, since) VALUES (?, ?, ?)
             ON CONFLICT (household_id, key) DO NOTHING",
            household_id,
            condition.key,
            since
        )
        .execute(&mut *conn)
        .await?
        .rows_affected();
        if inserted == 0 {
            return Ok(0);
        }
        let mut data = condition.data.clone();
        if let Value::Object(map) = &mut data {
            map.insert(
                "alert_key".to_string(),
                Value::String(condition.key.clone()),
            );
        }
        let event = NewEvent::new(
            &*self.clock,
            condition.entity_type,
            condition.action,
            condition.entity_id.clone(),
            &data,
        )?;
        append_event(&mut *conn, ctx, Origin::Scheduler, event).await?;
        Ok(1)
    }

    async fn has_state(&self, ctx: &HouseholdCtx, key: &str) -> Result<bool, AppError> {
        let household_id = ctx.household_id.to_string();
        let found = sqlx::query_scalar!(
            "SELECT 1 AS found FROM alert_states WHERE household_id = ? AND key = ?",
            household_id,
            key
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(found.is_some())
    }
}
