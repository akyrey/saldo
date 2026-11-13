//! The JSON shapes of bills and bill periods, used by the API and in events.

use saldo_core::{
    Bill, BillFrequency, BillPeriod, BillSchedule, Currency, CurrencyCode, Money, PeriodStatus,
};
use serde::Serialize;
use sqlx::SqliteConnection;

use crate::currencies;
use crate::error::{AppError, parse_stored};

/// A bill as the API returns it. Amounts are decimal strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct BillView {
    pub id: String,
    pub name: String,
    pub amount_min: String,
    pub amount_max: String,
    pub currency: String,
    pub first_due_date: String,
    pub end_date: Option<String>,
    /// `weekly`, `monthly`, `quarterly`, `half_year` or `yearly`.
    pub repeat_freq: String,
    pub skip: i64,
    pub remind_days_before: i64,
    pub remind_every_days: i64,
    pub active: bool,
    pub sort_order: i64,
    pub notes: Option<String>,
    /// The earliest open period's due date, if any.
    pub next_due_date: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One due date of a bill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct BillPeriodView {
    pub id: String,
    pub bill_id: String,
    pub due_date: String,
    /// `open`, `paid` or `skipped`.
    pub status: String,
    pub paid_split_id: Option<String>,
    pub paid_at: Option<String>,
    pub last_reminded_on: Option<String>,
    pub reminder_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// One `bills` row exactly as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BillRow {
    pub id: String,
    pub household_id: String,
    pub name: String,
    pub amount_min_minor: i64,
    pub amount_max_minor: i64,
    pub currency_code: String,
    pub first_due_date: String,
    pub end_date: Option<String>,
    pub repeat_freq: String,
    pub skip: i64,
    pub remind_days_before: i64,
    pub remind_every_days: i64,
    pub active: bool,
    pub sort_order: i64,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One `bill_periods` row as the API shows it (same columns).
pub(crate) type PeriodRow = BillPeriodView;

impl BillRow {
    /// The domain view of the stored bill, for schedules and reminders.
    pub(crate) fn domain(&self) -> Result<Bill, AppError> {
        let corrupt = |what: &str| AppError::Internal(format!("bad {what} on bill {}", self.id));
        let schedule = BillSchedule {
            first_due_date: parse_stored(&self.first_due_date)?,
            frequency: BillFrequency::parse(&self.repeat_freq)
                .ok_or_else(|| corrupt("repeat_freq"))?,
            skip: u32::try_from(self.skip).map_err(|_| corrupt("skip"))?,
            end_date: self.end_date.as_deref().map(parse_stored).transpose()?,
        };
        Ok(Bill {
            schedule,
            remind_days_before: u32::try_from(self.remind_days_before)
                .map_err(|_| corrupt("remind_days_before"))?,
            remind_every_days: u32::try_from(self.remind_every_days)
                .map_err(|_| corrupt("remind_every_days"))?,
            active: self.active,
        })
    }
}

impl PeriodRow {
    pub(crate) fn domain(&self) -> Result<BillPeriod, AppError> {
        let status = match self.status.as_str() {
            "open" => PeriodStatus::Open,
            "paid" => PeriodStatus::Paid,
            "skipped" => PeriodStatus::Skipped,
            other => return Err(AppError::Internal(format!("bad period status {other:?}"))),
        };
        Ok(BillPeriod {
            due_date: parse_stored(&self.due_date)?,
            status,
            last_reminded_on: self
                .last_reminded_on
                .as_deref()
                .map(parse_stored)
                .transpose()?,
        })
    }
}

/// The API view of a stored bill.
pub(crate) async fn view_of(
    conn: &mut SqliteConnection,
    row: BillRow,
) -> Result<BillView, AppError> {
    let currency = currencies::find(&mut *conn, &row.currency_code, "currency").await?;
    let next_due_date = sqlx::query_scalar!(
        "SELECT min(due_date) FROM bill_periods WHERE household_id = ? AND bill_id = ? AND status = 'open'",
        row.household_id,
        row.id,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(BillView {
        amount_min: format(row.amount_min_minor, &currency),
        amount_max: format(row.amount_max_minor, &currency),
        next_due_date,
        id: row.id,
        name: row.name,
        currency: row.currency_code,
        first_due_date: row.first_due_date,
        end_date: row.end_date,
        repeat_freq: row.repeat_freq,
        skip: row.skip,
        remind_days_before: row.remind_days_before,
        remind_every_days: row.remind_every_days,
        active: row.active,
        sort_order: row.sort_order,
        notes: row.notes,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn format(minor: i64, currency: &Currency) -> String {
    let code: CurrencyCode = currency.code();
    Money::new(minor, code).to_decimal_string(currency)
}
