//! PayPal's activity export, Italian locale, as xlsx or CSV (L5, L6, L23).

use chrono::{NaiveDate, NaiveTime};
use saldo_core::matching::{PayPalKind, classify_paypal};
use saldo_core::{Currency, Money};

use super::{SourceRow, currency_of, header_row, names};
use crate::statements::parse::table::{cell, find};
use crate::statements::parse::values::amount;
use crate::statements::parse::{Cell, ColumnNames, Format, ParseError, read_rows};

/// One PayPal row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayPalRecord {
    pub row: usize,
    pub date: NaiveDate,
    pub time: Option<NaiveTime>,
    pub timezone: Option<String>,
    /// `Nome`: the merchant or person.
    pub name: String,
    /// `Tipo`, as written.
    pub kind_text: String,
    pub kind: PayPalKind,
    pub status: String,
    /// `Importo` (`Lordo` in older exports), signed as PayPal writes it.
    pub gross: Money,
    pub fee: Option<Money>,
    pub net: Option<Money>,
    /// `Codice transazione`: the row's stable ID.
    pub transaction_code: String,
    pub receipt_code: Option<String>,
    pub description: Option<String>,
}

/// Column positions.
struct Columns {
    date: usize,
    code: usize,
    currency: usize,
    gross: usize,
    time: Option<usize>,
    timezone: Option<usize>,
    name: Option<usize>,
    kind: Option<usize>,
    status: Option<usize>,
    fee: Option<usize>,
    net: Option<usize>,
    receipt: Option<usize>,
    description: Option<usize>,
}

/// PayPal writes amounts as Italian text: "-204,37", "1.234,56".
const DECIMAL_SEPARATOR: char = ',';

/// Reads a PayPal export. `currencies` are the household's; a row in
/// another currency is unsupported.
pub fn parse_paypal(
    format: Format,
    bytes: &[u8],
    currencies: &[Currency],
) -> Result<Vec<SourceRow<PayPalRecord>>, ParseError> {
    let rows = read_rows(format, bytes, None)?;
    let required: [ColumnNames; 4] = [
        names(&["Data"]),
        names(&["Codice transazione"]),
        names(&["Valuta"]),
        names(&["Importo", "Lordo"]),
    ];
    let header_at = header_row(&rows, &required)?;
    let header = &rows[header_at];
    let at = |list: &[&str]| find(header, &names(list));
    let columns = Columns {
        date: at(&["Data"]).unwrap_or_default(),
        code: at(&["Codice transazione"]).unwrap_or_default(),
        currency: at(&["Valuta"]).unwrap_or_default(),
        gross: at(&["Importo", "Lordo"]).unwrap_or_default(),
        time: at(&["Orario"]),
        timezone: at(&["Fuso orario"]),
        name: at(&["Nome"]),
        kind: at(&["Tipo"]),
        status: at(&["Stato"]),
        fee: at(&["Tariffe"]),
        net: at(&["Totale"]),
        receipt: at(&["Codice ricevuta"]),
        description: at(&["Descrizione"]),
    };
    Ok(rows
        .iter()
        .enumerate()
        .skip(header_at + 1)
        .filter(|(_, row)| !row.iter().all(Cell::is_empty))
        .map(
            |(index, row)| match record(index + 1, row, &columns, currencies) {
                Ok(record) => SourceRow::Record(record),
                Err(reason) => SourceRow::Unsupported {
                    row: index + 1,
                    reason,
                },
            },
        )
        .collect())
}

fn text(row: &[Cell], at: Option<usize>) -> Option<String> {
    let value = cell(row, at?).text();
    (!value.is_empty()).then_some(value)
}

fn record(
    number: usize,
    row: &[Cell],
    columns: &Columns,
    currencies: &[Currency],
) -> Result<PayPalRecord, String> {
    let date = paypal_date(cell(row, columns.date))?;
    let currency = currency_of(&cell(row, columns.currency).text(), currencies)?;
    let read = |at: usize| {
        amount(cell(row, at), DECIMAL_SEPARATOR, currency)
            .map_err(|text| format!("unreadable amount {text:?}"))
    };
    let gross = read(columns.gross)?.ok_or_else(|| "no amount".to_string())?;
    let kind_text = text(row, columns.kind).unwrap_or_default();
    let status = text(row, columns.status).unwrap_or_default();
    let transaction_code =
        text(row, Some(columns.code)).ok_or_else(|| "no transaction code".to_string())?;
    Ok(PayPalRecord {
        row: number,
        date,
        time: columns.time.and_then(|at| paypal_time(cell(row, at))),
        timezone: text(row, columns.timezone),
        name: text(row, columns.name).unwrap_or_default(),
        kind: classify_paypal(&kind_text, &status, gross),
        kind_text,
        status,
        gross,
        fee: columns.fee.map(read).transpose()?.flatten(),
        net: columns.net.map(read).transpose()?.flatten(),
        transaction_code,
        receipt_code: text(row, columns.receipt),
        description: text(row, columns.description),
    })
}

/// A spreadsheet date, or text `d/M/yy` or `dd/mm/yyyy` (L6). The year's
/// length picks the format: chrono would read "25" as the year 25.
fn paypal_date(date_cell: &Cell) -> Result<NaiveDate, String> {
    let unreadable = || format!("unreadable date {:?}", date_cell.text());
    match date_cell {
        Cell::DateTime(moment) => Ok(moment.date()),
        Cell::Text(text) => {
            let text = text.trim();
            let year = text.rsplit('/').next().unwrap_or("");
            let format = if year.len() == 2 {
                "%d/%m/%y"
            } else {
                "%d/%m/%Y"
            };
            NaiveDate::parse_from_str(text, format).map_err(|_| unreadable())
        }
        Cell::Empty | Cell::Number(_) => Err(unreadable()),
    }
}

/// `Orario`: a spreadsheet time (a fraction of a day) or text `HH:MM:SS`.
fn paypal_time(time_cell: &Cell) -> Option<NaiveTime> {
    match time_cell {
        Cell::DateTime(moment) => Some(moment.time()),
        Cell::Text(text) => NaiveTime::parse_from_str(text.trim(), "%H:%M:%S")
            .or_else(|_| NaiveTime::parse_from_str(text.trim(), "%H:%M"))
            .ok(),
        Cell::Empty | Cell::Number(_) => None,
    }
}
