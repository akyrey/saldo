//! The due dates active bills still have, for the cash-flow forecast.

use chrono::{Days, NaiveDate};
use sqlx::SqliteConnection;

use super::periods::{all_rows, period_rows};
use crate::ctx::HouseholdCtx;
use crate::error::{AppError, parse_stored};

/// An active bill and its due dates not paid or skipped yet, up to `to`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpcomingBill {
    pub id: String,
    pub name: String,
    pub amount_max_minor: i64,
    pub currency_code: String,
    /// The account that paid the latest paid period, if any.
    pub payer_account_id: Option<String>,
    /// Open periods (overdue ones included), then the due dates after the
    /// periods created so far (they are only created 60 days ahead).
    pub dates: Vec<NaiveDate>,
}

pub(crate) async fn upcoming_in(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    to: NaiveDate,
) -> Result<Vec<UpcomingBill>, AppError> {
    let mut upcoming = Vec::new();
    for row in all_rows(&mut *conn, ctx)
        .await?
        .into_iter()
        .filter(|r| r.active)
    {
        let schedule = row.domain()?.schedule;
        let mut dates = Vec::new();
        for period in period_rows(&mut *conn, &row.id, Some("open")).await? {
            dates.push(parse_stored::<NaiveDate>(&period.due_date)?);
        }
        let from = match latest_due_date(&mut *conn, &row.id).await? {
            Some(latest) => latest.checked_add_days(Days::new(1)),
            None => Some(schedule.first_due_date),
        };
        if let Some(from) = from {
            dates.extend(schedule.occurrences(from, to));
        }
        dates.retain(|date| *date <= to);
        if dates.is_empty() {
            continue;
        }
        upcoming.push(UpcomingBill {
            payer_account_id: payer(&mut *conn, &row.id).await?,
            id: row.id,
            name: row.name,
            amount_max_minor: row.amount_max_minor,
            currency_code: row.currency_code,
            dates,
        });
    }
    Ok(upcoming)
}

async fn latest_due_date(
    conn: &mut SqliteConnection,
    bill_id: &str,
) -> Result<Option<NaiveDate>, AppError> {
    let latest = sqlx::query_scalar!(
        "SELECT max(due_date) FROM bill_periods WHERE bill_id = ?",
        bill_id
    )
    .fetch_one(&mut *conn)
    .await?;
    latest.as_deref().map(parse_stored).transpose()
}

/// The source account of the split that paid the bill's latest paid period.
async fn payer(conn: &mut SqliteConnection, bill_id: &str) -> Result<Option<String>, AppError> {
    let account = sqlx::query_scalar!(
        "SELECT s.source_account_id FROM bill_periods p JOIN splits s ON s.id = p.paid_split_id
         WHERE p.bill_id = ? AND p.status = 'paid'
         ORDER BY p.due_date DESC LIMIT 1",
        bill_id
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(account)
}
