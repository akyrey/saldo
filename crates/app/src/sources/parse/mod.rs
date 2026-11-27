//! Amazon and PayPal exports into records: pure functions over bytes, like
//! the statement parsers they share their cell reader with.

mod amazon;
mod paypal;

use saldo_core::Currency;

use crate::statements::parse::table::find;
use crate::statements::parse::{Cell, ColumnNames, ParseError};

pub use amazon::{AmazonRecord, parse_amazon};
pub use paypal::{PayPalRecord, parse_paypal};

/// One row of a source file that was not empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceRow<T> {
    Record(T),
    /// Not stored, with the reason shown to the owner (L7). `row` counts from
    /// 1 like the spreadsheet.
    Unsupported {
        row: usize,
        reason: String,
    },
}

fn names(list: &[&str]) -> ColumnNames {
    list.iter().map(|name| name.to_string()).collect()
}

/// The index of the header row: the first row naming every required column.
fn header_row(rows: &[Vec<Cell>], required: &[ColumnNames]) -> Result<usize, ParseError> {
    rows.iter()
        .position(|row| required.iter().all(|names| find(row, names).is_some()))
        .ok_or_else(|| {
            let missing = required
                .iter()
                .find(|names| rows.iter().all(|row| find(row, names).is_none()))
                .or(required.first())
                .cloned()
                .unwrap_or_default();
            ParseError::MissingColumn(missing)
        })
}

/// The currency of a row by its code, among the household's currencies.
fn currency_of<'a>(code: &str, currencies: &'a [Currency]) -> Result<&'a Currency, String> {
    currencies
        .iter()
        .find(|currency| {
            currency
                .code()
                .to_string()
                .eq_ignore_ascii_case(code.trim())
        })
        .ok_or_else(|| format!("unknown currency {}", code.trim()))
}
