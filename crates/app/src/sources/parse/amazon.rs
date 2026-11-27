//! Amazon's "Your Orders" data export: one CSV row per item (L10, L19–L22).

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate};
use chrono_tz::Tz;
use saldo_core::{Currency, Money};

use super::{SourceRow, currency_of, header_row, names};
use crate::statements::parse::table::{cell, find};
use crate::statements::parse::values::amount;
use crate::statements::parse::{Cell, ColumnNames, Format, ParseError, read_rows};

/// One item of an order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmazonRecord {
    pub row: usize,
    /// `order:ASIN:n`, n counting the same product in the same order, so a
    /// newer export of the same item maps to the same stored row.
    pub external_id: String,
    pub order_id: String,
    pub asin: String,
    /// The order date in the household's time zone (L10).
    pub date: NaiveDate,
    /// `Total Amount`: what the item cost, tax included.
    pub amount: Money,
    pub product: String,
    pub status: String,
    pub payment_method: String,
    pub website: String,
    pub cancelled: bool,
    /// Paid by gift card alone: never reaches a bank (L22).
    pub gift_card_only: bool,
}

struct Columns {
    order: usize,
    date: usize,
    currency: usize,
    total: usize,
    product: Option<usize>,
    status: Option<usize>,
    method: Option<usize>,
    asin: Option<usize>,
    website: Option<usize>,
}

/// Reads an Amazon order history CSV; dates are converted to `zone`.
pub fn parse_amazon(
    bytes: &[u8],
    currencies: &[Currency],
    zone: Tz,
) -> Result<Vec<SourceRow<AmazonRecord>>, ParseError> {
    let rows = read_rows(Format::Csv, bytes, None)?;
    let required: [ColumnNames; 4] = [
        names(&["Order ID"]),
        names(&["Order Date"]),
        names(&["Currency"]),
        names(&["Total Amount", "Total Owed"]),
    ];
    let header_at = header_row(&rows, &required)?;
    let header = &rows[header_at];
    let at = |list: &[&str]| find(header, &names(list));
    let columns = Columns {
        order: at(&["Order ID"]).unwrap_or_default(),
        date: at(&["Order Date"]).unwrap_or_default(),
        currency: at(&["Currency"]).unwrap_or_default(),
        total: at(&["Total Amount", "Total Owed"]).unwrap_or_default(),
        product: at(&["Product Name"]),
        status: at(&["Order Status"]),
        method: at(&["Payment Method Type", "Payment Instrument Type"]),
        asin: at(&["ASIN"]),
        website: at(&["Website"]),
    };

    let mut seen: HashMap<(String, String), u32> = HashMap::new();
    let mut parsed = Vec::new();
    for (index, row) in rows.iter().enumerate().skip(header_at + 1) {
        if row.iter().all(Cell::is_empty) {
            continue;
        }
        match record(index + 1, row, &columns, currencies, zone) {
            Ok(mut record) => {
                let count = seen
                    .entry((record.order_id.clone(), record.asin.clone()))
                    .or_insert(0);
                *count += 1;
                record.external_id = format!("{}:{}:{count}", record.order_id, record.asin);
                parsed.push(SourceRow::Record(record));
            }
            Err(reason) => parsed.push(SourceRow::Unsupported {
                row: index + 1,
                reason,
            }),
        }
    }
    Ok(parsed)
}

fn text(row: &[Cell], at: Option<usize>) -> String {
    at.map(|at| cell(row, at).text()).unwrap_or_default()
}

fn record(
    number: usize,
    row: &[Cell],
    columns: &Columns,
    currencies: &[Currency],
    zone: Tz,
) -> Result<AmazonRecord, String> {
    let order_id = text(row, Some(columns.order));
    if order_id.is_empty() {
        return Err("no order ID".to_string());
    }
    let date = order_date(&text(row, Some(columns.date)), zone)?;
    let currency = currency_of(&text(row, Some(columns.currency)), currencies)?;
    let total = text(row, Some(columns.total));
    // Exports use a dot; some locales a comma. A lone comma is a decimal.
    let separator = if total.contains(',') && !total.contains('.') {
        ','
    } else {
        '.'
    };
    let amount = amount(cell(row, columns.total), separator, currency)
        .map_err(|text| format!("unreadable amount {text:?}"))?
        .ok_or_else(|| "no amount".to_string())?;
    let status = text(row, columns.status);
    let payment_method = text(row, columns.method);
    let method = payment_method.to_lowercase();
    Ok(AmazonRecord {
        row: number,
        external_id: String::new(),
        asin: text(row, columns.asin),
        date,
        amount,
        product: text(row, columns.product),
        cancelled: status.to_lowercase().contains("cancel"),
        // "Gift Certificate/Card" alone; a card with digits paid at least part.
        gift_card_only: method.contains("gift") && !method.chars().any(|c| c.is_ascii_digit()),
        status,
        payment_method,
        website: text(row, columns.website),
        order_id,
    })
}

/// ISO 8601 with a zone (`2024-01-05T23:30:00Z`) in `zone`, or a plain date.
fn order_date(text: &str, zone: Tz) -> Result<NaiveDate, String> {
    DateTime::parse_from_rfc3339(text)
        .map(|moment| moment.with_timezone(&zone).date_naive())
        .or_else(|_| NaiveDate::parse_from_str(text, "%Y-%m-%d"))
        .map_err(|_| format!("unreadable order date {text:?}"))
}
