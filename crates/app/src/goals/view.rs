//! The goal as the API returns it, with its progress computed when read.

use chrono::{Datelike, Months, NaiveDate};
use saldo_core::goal::progress;
use saldo_core::ratios::monthly_average;
use saldo_core::{Currency, Money, UserId};
use serde::Serialize;
use sqlx::SqliteConnection;

use crate::ctx::HouseholdCtx;
use crate::currencies;
use crate::error::{AppError, parse_stored};
use crate::reports::{Interval, NetWorthReport, ReportQuery, net_worth_in};
use crate::scope::Scope;

/// How many complete months the average monthly change looks back.
pub const TREND_MONTHS: u32 = 12;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct NetWorthGoalView {
    pub id: String,
    pub name: String,
    /// The member whose accounts count; `null` for the whole household.
    pub owner_user_id: Option<String>,
    pub target_amount: String,
    /// Net worth when the goal was created.
    pub start_amount: String,
    pub currency: String,
    pub target_date: Option<String>,
    /// The day the alert scheduler saw it reached.
    pub reached_on: Option<String>,
    pub active: bool,
    pub notes: Option<String>,
    pub progress: GoalProgressView,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct GoalProgressView {
    /// Net worth today (the goal's scope and currency).
    pub current: String,
    pub remaining: String,
    /// 0 to 10 000 (100%).
    pub progress_bp: i64,
    /// What each month until the target date must add; `null` without one.
    pub required_monthly: Option<String>,
    /// The average monthly change over the last 12 complete months.
    pub average_monthly_change: String,
    /// When that average would reach the target; `null` when it does not.
    pub projected_date: Option<String>,
    pub on_track: bool,
    pub reached: bool,
}

pub(super) struct Row {
    pub id: String,
    pub name: String,
    pub owner_user_id: Option<String>,
    pub target_amount_minor: i64,
    pub start_amount_minor: i64,
    pub currency_code: String,
    pub target_date: Option<String>,
    pub reached_on: Option<String>,
    pub active: bool,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// The goal's scope: one member's accounts, or the whole household's.
pub(super) fn scope_of(owner: Option<&str>) -> Result<Scope, AppError> {
    match owner {
        Some(id) => Ok(Scope::User(parse_stored::<UserId>(id)?)),
        None => Ok(Scope::Household),
    }
}

/// Net worth today and the average monthly change, in `currency`.
pub(super) async fn trend(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    scope: Scope,
    currency: &Currency,
    today: NaiveDate,
) -> Result<(i64, i64), AppError> {
    let this_month = today.with_day(1).unwrap_or(today);
    let from = this_month
        .checked_sub_months(Months::new(TREND_MONTHS))
        .unwrap_or(this_month);
    let query = ReportQuery {
        from,
        to: today,
        interval: Interval::Month,
        scope,
    };
    let report = net_worth_in(conn, ctx, &query).await?;
    trend_in(&report, currency)
}

fn trend_in(report: &NetWorthReport, currency: &Currency) -> Result<(i64, i64), AppError> {
    let code = currency.code().to_string();
    let Some(series) = report.currencies.iter().find(|c| c.currency == code) else {
        return Ok((0, 0));
    };
    let minor = |text: &str| {
        Money::parse_decimal(text, currency)
            .map(|m| m.minor())
            .map_err(|e| AppError::Internal(format!("bad net worth {text:?}: {e}")))
    };
    let current = match series.intervals.last() {
        Some(last) => minor(&last.value)?,
        None => minor(&series.opening)?,
    };
    // Complete months only: the last interval is the current, partial one.
    let complete = series.intervals.len().saturating_sub(1);
    let changes = series.intervals[..complete]
        .iter()
        .map(|i| minor(&i.change))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((current, monthly_average(&changes)))
}

pub(super) async fn view(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    row: Row,
    today: NaiveDate,
) -> Result<NetWorthGoalView, AppError> {
    let currency = currencies::find(&mut *conn, &row.currency_code, "currency").await?;
    let scope = scope_of(row.owner_user_id.as_deref())?;
    let (current, pace) = trend(&mut *conn, ctx, scope, &currency, today).await?;
    let target_date: Option<NaiveDate> =
        row.target_date.as_deref().map(parse_stored).transpose()?;
    let p = progress(
        row.start_amount_minor,
        current,
        row.target_amount_minor,
        today,
        target_date,
        pace,
    );
    let money = |minor: i64| Money::new(minor, currency.code()).to_decimal_string(&currency);
    Ok(NetWorthGoalView {
        progress: GoalProgressView {
            current: money(current),
            remaining: money(p.remaining),
            progress_bp: p.progress_bp,
            required_monthly: p.required_monthly.map(money),
            average_monthly_change: money(pace),
            projected_date: p.projected_date.map(|d| d.to_string()),
            on_track: p.on_track,
            reached: p.reached,
        },
        target_amount: money(row.target_amount_minor),
        start_amount: money(row.start_amount_minor),
        id: row.id,
        name: row.name,
        owner_user_id: row.owner_user_id,
        currency: row.currency_code,
        target_date: row.target_date,
        reached_on: row.reached_on,
        active: row.active,
        notes: row.notes,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
