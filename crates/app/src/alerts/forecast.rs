//! The forecast condition: `forecast:<account id>` while an asset account in
//! net worth is projected below zero within the next 30 days.

use chrono::{Days, NaiveDate};
use saldo_core::{AccountId, Money};
use serde_json::json;
use sqlx::SqlitePool;

use super::{Condition, FORECAST_ALERT_DAYS};
use crate::ctx::HouseholdCtx;
use crate::currencies;
use crate::error::{AppError, parse_stored};
use crate::reports::{ForecastQuery, Interval, ReportService};
use crate::scope::Scope;

pub(super) async fn conditions(
    pool: &SqlitePool,
    reports: &ReportService,
    ctx: &HouseholdCtx,
    today: NaiveDate,
) -> Result<Vec<Condition>, AppError> {
    let household_id = ctx.household_id.to_string();
    let accounts = sqlx::query!(
        "SELECT id, name FROM accounts WHERE household_id = ? AND type = 'asset'
           AND active = 1 AND include_net_worth = 1 ORDER BY name",
        household_id
    )
    .fetch_all(pool)
    .await?;
    let to = today + Days::new(FORECAST_ALERT_DAYS);
    let mut conditions = Vec::new();
    for account in accounts {
        let query = ForecastQuery {
            to,
            interval: Interval::Week,
            scope: Scope::Household,
            account_id: Some(parse_stored::<AccountId>(&account.id)?),
        };
        let report = reports.forecast(ctx, today, &query).await?;
        for projected in report.currencies {
            if !below_zero(pool, &projected.currency, &projected.lowest.balance).await? {
                continue;
            }
            conditions.push(Condition {
                key: format!("forecast:{}", account.id),
                entity_type: "forecast",
                action: "below_zero",
                entity_id: account.id.clone(),
                data: json!({
                    "account": { "id": account.id, "name": account.name },
                    "currency": projected.currency,
                    "lowest": projected.lowest,
                }),
            });
        }
    }
    Ok(conditions)
}

async fn below_zero(pool: &SqlitePool, code: &str, balance: &str) -> Result<bool, AppError> {
    let mut conn = pool.acquire().await?;
    let currency = currencies::find(&mut conn, code, "currency").await?;
    let money = Money::parse_decimal(balance, &currency)
        .map_err(|e| AppError::Internal(format!("bad forecast balance {balance:?}: {e}")))?;
    Ok(money.minor() < 0)
}
