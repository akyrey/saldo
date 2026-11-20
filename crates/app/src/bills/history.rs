//! A bill's history (Phase 11): every split linked to it, its due dates over
//! the bill's whole duration, and a summary of what was paid
//! (`GET /bills/{id}/history`). Read-only: no events.

use std::collections::HashMap;

use chrono::{Days, NaiveDate};
use saldo_core::bill_history::{HistorySpan, SpanInput, due_status, history_span, payment_stats};
use saldo_core::{BillId, BillSchedule, Currency};
use serde::Serialize;
use sqlx::SqliteConnection;

use super::periods::period_rows;
use super::view::{BillRow, PeriodRow};
use super::{BillService, find, minor_text};
use crate::ctx::HouseholdCtx;
use crate::currencies;
use crate::error::{AppError, parse_stored};
use crate::identity::today_in;

/// At most this many due dates are returned; older ones are dropped.
pub const MAX_DUE_DATES: usize = 2000;

/// The dates a history covers, both included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct HistoryRange {
    pub from: String,
    pub to: String,
}

/// One due date in a bill's history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct HistoryDueDate {
    pub date: String,
    /// `paid`, `skipped`, `open`, `overdue` or `scheduled` (no period yet).
    pub status: String,
    pub period_id: Option<String>,
    pub paid_split_id: Option<String>,
}

/// An account as a payment shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct HistoryAccount {
    pub id: String,
    pub name: String,
}

/// A split linked to the bill, by `bill_id` or by paying one of its periods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct HistoryPayment {
    pub split_id: String,
    pub group_id: String,
    pub date: String,
    pub description: String,
    /// A decimal string in `currency`, which may differ from the bill's.
    pub amount: String,
    pub currency: String,
    pub source: HistoryAccount,
    pub destination: HistoryAccount,
    /// The due date this split paid, if any.
    pub due_date: Option<String>,
}

/// Payments in the bill's currency, summarised. Amounts are `null` without
/// any such payment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct HistorySummary {
    pub count: i64,
    pub total: Option<String>,
    pub average: Option<String>,
    pub min: Option<String>,
    pub max: Option<String>,
    /// Linked payments in another currency, left out of the figures above.
    pub other_currency_count: i64,
}

/// A bill's history as the API returns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct BillHistoryView {
    pub bill_id: String,
    pub currency: String,
    pub range: HistoryRange,
    pub amount_min: String,
    pub amount_max: String,
    /// Oldest first.
    pub due_dates: Vec<HistoryDueDate>,
    /// Newest first.
    pub payments: Vec<HistoryPayment>,
    pub summary: HistorySummary,
    /// Whether older due dates were left out (more than 2 000).
    pub truncated: bool,
}

/// One linked split as stored, with its account names.
struct PaymentRow {
    id: String,
    group_id: String,
    date: String,
    description: String,
    amount_minor: i64,
    currency_code: String,
    source_account_id: String,
    source_name: String,
    destination_account_id: String,
    destination_name: String,
}

impl BillService {
    /// The history of one of the household's bills, up to today in the
    /// household's timezone (and beyond, for the due dates ahead).
    pub async fn history(
        &self,
        ctx: &HouseholdCtx,
        id: BillId,
    ) -> Result<BillHistoryView, AppError> {
        let mut conn = self.pool.acquire().await?;
        let bill = find(&mut conn, ctx, id).await?;
        let today = today_in(&mut conn, ctx, &*self.clock).await?;
        let periods = period_rows(&mut conn, &bill.id, None).await?;
        let payments = payment_rows(&mut conn, ctx, &bill.id).await?;
        let schedule = bill.domain()?.schedule;
        let span = span_of(&bill, &periods, &payments, today)?;
        let (due_dates, truncated) = due_dates(&periods, &schedule, span, today)?;
        let bill_currency = currencies::find(&mut conn, &bill.currency_code, "currency").await?;
        let summary = summary_of(&payments, &bill_currency)?;
        let payments = payment_views(&mut conn, payments, &periods).await?;
        Ok(BillHistoryView {
            range: HistoryRange {
                from: span.from.to_string(),
                to: span.to.to_string(),
            },
            amount_min: minor_text(bill.amount_min_minor, &bill_currency),
            amount_max: minor_text(bill.amount_max_minor, &bill_currency),
            bill_id: bill.id,
            currency: bill.currency_code,
            due_dates,
            payments,
            summary,
            truncated,
        })
    }
}

/// Splits whose `bill_id` is the bill, or that paid one of its periods
/// (`POST /bill-periods/{id}/pay` does not set `bill_id`). Newest first.
async fn payment_rows(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    bill_id: &str,
) -> Result<Vec<PaymentRow>, AppError> {
    let household_id = ctx.household_id.to_string();
    let rows = sqlx::query_as!(
        PaymentRow,
        "SELECT s.id, s.group_id, s.date, s.description, s.amount_minor, s.currency_code,
                s.source_account_id, src.name AS source_name,
                s.destination_account_id, dst.name AS destination_name
         FROM splits s
         JOIN accounts src ON src.id = s.source_account_id
         JOIN accounts dst ON dst.id = s.destination_account_id
         WHERE s.household_id = ?1
           AND (s.bill_id = ?2
                OR s.id IN (SELECT paid_split_id FROM bill_periods
                            WHERE household_id = ?1 AND bill_id = ?2 AND paid_split_id IS NOT NULL))
         ORDER BY s.date DESC, s.id",
        household_id,
        bill_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

fn span_of(
    bill: &BillRow,
    periods: &[PeriodRow],
    payments: &[PaymentRow],
    today: NaiveDate,
) -> Result<HistorySpan, AppError> {
    // Both lists are sorted: periods oldest first, payments newest first.
    let last_period_due = periods
        .last()
        .map(|p| parse_stored(&p.due_date))
        .transpose()?;
    let earliest_payment = payments.last().map(|p| parse_stored(&p.date)).transpose()?;
    Ok(history_span(SpanInput {
        first_due_date: parse_stored(&bill.first_due_date)?,
        end_date: bill.end_date.as_deref().map(parse_stored).transpose()?,
        last_period_due,
        earliest_payment,
        today,
    }))
}

/// The stored periods inside the span, then the scheduled due dates after
/// the last one, oldest first; at most [`MAX_DUE_DATES`] (the latest).
fn due_dates(
    periods: &[PeriodRow],
    schedule: &BillSchedule,
    span: HistorySpan,
    today: NaiveDate,
) -> Result<(Vec<HistoryDueDate>, bool), AppError> {
    let mut dates = Vec::new();
    for period in periods {
        let due: NaiveDate = parse_stored(&period.due_date)?;
        if due < span.from || due > span.to {
            continue;
        }
        let stored = period.domain()?.status;
        dates.push(HistoryDueDate {
            date: period.due_date.clone(),
            status: due_status(Some(stored), due, today).as_str().to_string(),
            period_id: Some(period.id.clone()),
            paid_split_id: period.paid_split_id.clone(),
        });
    }
    let after_last = match periods.last() {
        Some(last) => parse_stored::<NaiveDate>(&last.due_date)?.checked_add_days(Days::new(1)),
        None => Some(span.from),
    };
    if let Some(from) = after_last {
        for due in schedule.occurrences(from.max(span.from), span.to) {
            dates.push(HistoryDueDate {
                date: due.to_string(),
                status: due_status(None, due, today).as_str().to_string(),
                period_id: None,
                paid_split_id: None,
            });
        }
    }
    let truncated = dates.len() > MAX_DUE_DATES;
    if truncated {
        dates.drain(..dates.len() - MAX_DUE_DATES);
    }
    Ok((dates, truncated))
}

fn summary_of(payments: &[PaymentRow], currency: &Currency) -> Result<HistorySummary, AppError> {
    let code = currency.code().to_string();
    let amounts: Vec<i64> = payments
        .iter()
        .filter(|p| p.currency_code == code)
        .map(|p| p.amount_minor)
        .collect();
    let other_currency_count = count(payments.len() - amounts.len());
    let stats = payment_stats(&amounts)
        .map_err(|e| AppError::Internal(format!("bill payment total: {e}")))?;
    let text = |minor: i64| Some(minor_text(minor, currency));
    Ok(match stats {
        Some(s) => HistorySummary {
            count: count(s.count),
            total: text(s.total),
            average: text(s.average),
            min: text(s.min),
            max: text(s.max),
            other_currency_count,
        },
        None => HistorySummary {
            count: 0,
            total: None,
            average: None,
            min: None,
            max: None,
            other_currency_count,
        },
    })
}

/// A length as the API's integer type (lengths always fit).
fn count(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

async fn payment_views(
    conn: &mut SqliteConnection,
    rows: Vec<PaymentRow>,
    periods: &[PeriodRow],
) -> Result<Vec<HistoryPayment>, AppError> {
    // Split ID → the due date it paid; built once, looked up per row.
    let paid: HashMap<&str, &str> = periods
        .iter()
        .filter_map(|p| Some((p.paid_split_id.as_deref()?, p.due_date.as_str())))
        .collect();
    let mut cache: HashMap<String, Currency> = HashMap::new();
    let mut views = Vec::with_capacity(rows.len());
    for row in rows {
        if !cache.contains_key(&row.currency_code) {
            let found = currencies::find(&mut *conn, &row.currency_code, "currency").await?;
            cache.insert(row.currency_code.clone(), found);
        }
        let currency = cache
            .get(&row.currency_code)
            .ok_or_else(|| AppError::Internal("currency cache".into()))?;
        views.push(HistoryPayment {
            due_date: paid.get(row.id.as_str()).map(|d| d.to_string()),
            amount: minor_text(row.amount_minor, currency),
            split_id: row.id,
            group_id: row.group_id,
            date: row.date,
            description: row.description,
            currency: row.currency_code,
            source: HistoryAccount {
                id: row.source_account_id,
                name: row.source_name,
            },
            destination: HistoryAccount {
                id: row.destination_account_id,
                name: row.destination_name,
            },
        });
    }
    Ok(views)
}
