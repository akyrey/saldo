//! The dates active recurrences will still fire, with their template splits,
//! for the cash-flow forecast.

use chrono::{Days, NaiveDate};
use saldo_core::recurrence::occurrences;
use sqlx::SqliteConnection;

use super::input::stored_schedule;
use super::repo;
use crate::ctx::HouseholdCtx;
use crate::error::{AppError, parse_stored};

/// One template split: what each firing books.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpcomingSplit {
    pub source_account_id: String,
    pub destination_account_id: String,
    pub amount_minor: i64,
    pub currency_code: String,
    pub foreign_amount_minor: Option<i64>,
    pub foreign_currency_code: Option<String>,
    pub bill_id: Option<String>,
}

/// An active recurrence and the dates it has not fired yet, up to `to`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpcomingRecurrence {
    pub id: String,
    pub title: String,
    pub dates: Vec<NaiveDate>,
    pub splits: Vec<UpcomingSplit>,
}

/// Every active recurrence with the dates after its `fired_through` up to
/// `to` (`max_occurrences` and `repeat_until` honoured by the schedule).
/// Dates not fired yet that are already past stay in the list: the runner
/// fires them within the hour, and the caller decides where to put them.
pub(crate) async fn upcoming_in(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    to: NaiveDate,
) -> Result<Vec<UpcomingRecurrence>, AppError> {
    let household_id = ctx.household_id.to_string();
    // SQLite reads `LIMIT -1` as "no limit".
    let (rows, _) = repo::list(&mut *conn, &household_id, Some(true), -1, 0).await?;
    let mut upcoming = Vec::with_capacity(rows.len());
    for row in rows {
        let repetitions = repo::repetitions(&mut *conn, &household_id, &row.id).await?;
        let schedule = stored_schedule(&row, &repetitions)?;
        let from = match row.fired_through.as_deref() {
            Some(done) => match parse_stored::<NaiveDate>(done)?.checked_add_days(Days::new(1)) {
                Some(next) => next,
                None => continue,
            },
            None => schedule.first_date,
        };
        let dates = occurrences(&schedule, from, to);
        if dates.is_empty() {
            continue;
        }
        let splits = repo::splits(&mut *conn, &household_id, &row.id)
            .await?
            .into_iter()
            .map(|s| UpcomingSplit {
                source_account_id: s.source_account_id,
                destination_account_id: s.destination_account_id,
                amount_minor: s.amount_minor,
                currency_code: s.currency_code,
                foreign_amount_minor: s.foreign_amount_minor,
                foreign_currency_code: s.foreign_currency_code,
                bill_id: s.bill_id,
            })
            .collect();
        upcoming.push(UpcomingRecurrence {
            id: row.id,
            title: row.title,
            dates,
            splits,
        });
    }
    Ok(upcoming)
}
