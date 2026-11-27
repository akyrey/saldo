//! Loading the stored Amazon and PayPal rows a statement may explain, and
//! asking `enrich` about them (Phase 13, Track D).

use chrono::{Days, NaiveDate, NaiveTime};
use saldo_core::matching::find_order_ids;
use saldo_core::{Currency, CurrencyCode, DateRange, Money};
use sqlx::SqliteConnection;

use super::enrich::{Enrichment, SourceRowData, enrich};
use super::ledger_repo::{self, SourceRowRecord};
use super::parse::StatementLine;
use crate::error::AppError;

/// Source rows are loaded from this long before the statement's first line
/// (an Amazon charge comes up to two weeks after the order) ...
const BEFORE_DAYS: u64 = 30;
/// ... to this long after its last (a bank line may precede its PayPal row).
const AFTER_DAYS: u64 = 3;

/// What the household's stored sources say about each line.
pub(super) async fn enrichments(
    conn: &mut SqliteConnection,
    household_id: &str,
    lines: &[StatementLine],
    currency: &Currency,
) -> Result<Vec<Option<Enrichment>>, AppError> {
    let (Some(first), Some(last)) = (
        lines.iter().map(|l| l.booked_date).min(),
        lines.iter().map(|l| l.booked_date).max(),
    ) else {
        return Ok(vec![None; lines.len()]);
    };
    let from = first
        .checked_sub_days(Days::new(BEFORE_DAYS))
        .unwrap_or(first);
    let to = last.checked_add_days(Days::new(AFTER_DAYS)).unwrap_or(last);
    let order_ids: Vec<String> = lines
        .iter()
        .flat_map(|l| find_order_ids(&l.description))
        .collect();
    let records = ledger_repo::source_rows(
        conn,
        household_id,
        &from.to_string(),
        &to.to_string(),
        &order_ids,
    )
    .await?;
    let mut amazon = Vec::new();
    let mut paypal = Vec::new();
    for record in records {
        let kind = record.kind.clone();
        let data = row_data(record)?;
        if kind == "amazon_orders" {
            amazon.push(data)
        } else {
            paypal.push(data)
        }
    }
    let range =
        DateRange::new(first, last).ok_or_else(|| AppError::Internal("empty range".to_string()))?;
    Ok(enrich(lines, &amazon, &paypal, range, currency))
}

fn row_data(record: SourceRowRecord) -> Result<SourceRowData, AppError> {
    let bad = |what: &str| AppError::Internal(format!("stored source row {}: {what}", record.id));
    let date: NaiveDate = record.date.parse().map_err(|_| bad("date"))?;
    let time = match &record.time {
        Some(text) => Some(NaiveTime::parse_from_str(text, "%H:%M:%S").map_err(|_| bad("time"))?),
        None => None,
    };
    let code = CurrencyCode::new(&record.currency_code).map_err(|_| bad("currency"))?;
    Ok(SourceRowData {
        id: record.id,
        external_id: record.external_id,
        date,
        time,
        amount: Money::new(record.amount_minor, code),
        row_kind: record.row_kind,
        group_key: record.group_key,
        counterparty: record.counterparty,
        text: record.text,
        excluded: record.excluded,
    })
}
