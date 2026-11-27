//! Statement files into lines: pure functions over bytes, no database and
//! no clock (Phase 13, Track B).
//!
//! Both formats first become rows of [`Cell`]s; one mapping step then turns
//! rows into lines, so CSV and xlsx share every rule. A broken *file* (not a
//! spreadsheet, no header, a missing column) is a [`ParseError`]; a broken
//! *row* becomes [`ParsedRow::Unsupported`] with a reason, never silently
//! dropped (L7).

mod cells;
mod mapping;
pub(crate) mod table;
pub(crate) mod values;

use chrono::NaiveDate;
use saldo_core::{Currency, Money};

pub use cells::Cell;
pub use mapping::{AmountColumns, BookOn, ColumnNames, DateRule, Mapping, MappingError, SkipRule};

/// Largest upload accepted.
pub const MAX_FILE_BYTES: usize = 5 * 1024 * 1024;
/// Largest number of rows read from one file.
pub const MAX_ROWS: usize = 10_000;
/// Largest uncompressed size of an xlsx, so a small zip cannot expand into
/// gigabytes.
pub const MAX_XLSX_EXPANDED_BYTES: u64 = 50 * 1024 * 1024;
/// Most columns a row may use; bank exports have about ten.
pub const MAX_COLUMNS: usize = 64;
/// Longest text kept from one cell; the rest is cut off.
pub const MAX_CELL_CHARS: usize = 2_000;
/// Most text read from one xlsx, across all cells.
pub const MAX_TEXT_BYTES: usize = 10 * 1024 * 1024;
/// Most shared strings an xlsx may declare (calamine reserves that many).
pub const MAX_SHARED_STRINGS: u64 = 1_000_000;

/// The file formats a profile reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Csv,
    Xlsx,
}

/// Why a whole file could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("the file is larger than {limit} bytes")]
    TooLarge { limit: usize },
    #[error("the file has more than {limit} rows")]
    TooManyRows { limit: usize },
    #[error("a row has more than {limit} columns")]
    TooManyColumns { limit: usize },
    #[error("the file holds more than {limit} bytes of text")]
    TooMuchText { limit: usize },
    #[error("the profile's mapping is not valid: {0}")]
    InvalidMapping(String),
    #[error("the spreadsheet expands to more than {limit} bytes")]
    TooLargeExpanded { limit: u64 },
    #[error("the file cannot be read: {0}")]
    Unreadable(String),
    #[error("no sheet named {0:?}")]
    NoSheet(String),
    #[error("no header row with a column named {0:?}")]
    HeaderNotFound(Vec<String>),
    #[error("the header row has no column named {0:?}")]
    MissingColumn(Vec<String>),
    #[error("the value next to {label:?} is not an amount: {value:?}")]
    BadBalance { label: String, value: String },
}

/// Why one row is not imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unsupported {
    /// No date: a total, a subtitle, a note under the table.
    NotATransaction,
    /// The bank has not booked it yet (decision 12).
    Pending,
    /// The amount is zero (a Satispay payment made with vouchers only).
    NothingMoved,
    /// In another currency than the account's.
    OtherCurrency(String),
    /// Left out by a skip rule, with the rule's reason.
    Skipped(String),
    UnreadableDate(String),
    UnreadableAmount(String),
}

/// A statement line ready for matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementLine {
    /// The row in the file, counting from 1 like the spreadsheet does.
    pub row: usize,
    pub accounting_date: NaiveDate,
    pub value_date: NaiveDate,
    /// The date the transaction gets: the accounting date, unless a date
    /// rule moved it (L8).
    pub booked_date: NaiveDate,
    /// Signed for the account: negative when money left it. Zero only when
    /// `parts` paid it all (a Satispay payment made with vouchers).
    pub amount: Money,
    /// The bank's text, kept verbatim apart from joining columns.
    pub description: String,
    pub category: Option<String>,
    pub counterparty: Option<String>,
    pub counterparty_iban: Option<String>,
    pub external_id: Option<String>,
    pub notes: Vec<String>,
    /// Parts of the same payment that left *other* accounts, such as
    /// Satispay's meal vouchers; the import turns each into a split from
    /// the account the profile assigns to its column.
    pub parts: Vec<LinePart>,
}

/// An amount from a part column (see [`Mapping::part_columns`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinePart {
    /// The column's header as the file writes it ("Meal Vouchers").
    pub column: String,
    /// Signed, non-zero: negative when money left that account.
    pub amount: Money,
}

/// One row of the file that was neither empty nor above the header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedRow {
    Line(StatementLine),
    Unsupported { row: usize, reason: Unsupported },
}

/// What a statement file holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedStatement {
    /// In file order.
    pub rows: Vec<ParsedRow>,
    pub opening_balance: Option<Money>,
    pub closing_balance: Option<Money>,
}

/// Reads a statement file with `mapping` for an account in `currency`.
pub fn parse_statement(
    format: Format,
    bytes: &[u8],
    mapping: &Mapping,
    currency: &Currency,
) -> Result<ParsedStatement, ParseError> {
    // Also checked when a profile is saved; checked here too so a mapping
    // read back from storage can never skip it.
    mapping
        .validate()
        .map_err(|error| ParseError::InvalidMapping(error.to_string()))?;
    let rows = read_rows(format, bytes, mapping.sheet.as_deref())?;
    table::read_statement(&rows, mapping, currency)
}

/// Reads a file into rows of cells, enforcing the size limits.
pub fn read_rows(
    format: Format,
    bytes: &[u8],
    sheet: Option<&str>,
) -> Result<Vec<Vec<Cell>>, ParseError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ParseError::TooLarge {
            limit: MAX_FILE_BYTES,
        });
    }
    let rows = match format {
        Format::Csv => cells::read_csv(bytes)?,
        Format::Xlsx => cells::read_xlsx(bytes, sheet)?,
    };
    if rows.len() > MAX_ROWS {
        return Err(ParseError::TooManyRows { limit: MAX_ROWS });
    }
    Ok(rows)
}
