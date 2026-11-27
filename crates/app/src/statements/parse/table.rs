//! Rows of cells into statement lines, following a [`Mapping`].

use chrono::{Datelike, NaiveDate};
use saldo_core::{Currency, Money};

use super::mapping::{AmountColumns, BookOn, ColumnNames};
use super::values::{amount, date};
use super::{
    Cell, LinePart, Mapping, ParseError, ParsedRow, ParsedStatement, StatementLine, Unsupported,
};

/// Column positions in the header row, resolved once.
struct Columns {
    date: usize,
    value_date: Option<usize>,
    amount: AmountAt,
    description: Vec<usize>,
    category: Option<usize>,
    counterparty: Option<usize>,
    counterparty_iban: Option<usize>,
    external_id: Option<usize>,
    currency: Option<usize>,
    /// `(header text, position)`.
    notes: Vec<(String, usize)>,
    /// `(header text, position)`.
    parts: Vec<(String, usize)>,
    running_balance: Option<usize>,
    /// `(position, lowercase text, reason)`.
    skip: Vec<(usize, String, String)>,
}

enum AmountAt {
    Signed(usize),
    Split { credit: usize, debit: usize },
}

pub fn read_statement(
    rows: &[Vec<Cell>],
    mapping: &Mapping,
    currency: &Currency,
) -> Result<ParsedStatement, ParseError> {
    // The header names the date and the amount: a prelude cell that merely
    // reads like the date column is not it.
    let amount_names = match &mapping.amount {
        AmountColumns::Signed { column } => column,
        AmountColumns::Split { credit, .. } => credit,
    };
    let header_index = rows
        .iter()
        .position(|row| {
            find(row, &mapping.date_column).is_some() && find(row, amount_names).is_some()
        })
        .ok_or_else(|| ParseError::HeaderNotFound(mapping.date_column.clone()))?;
    let columns = resolve(&rows[header_index], mapping)?;

    let (parsed, running) = read_body(rows, header_index, &columns, mapping, currency);

    let (mut opening_balance, mut closing_balance) = (None, None);
    if let Some(label) = &mapping.opening_balance_label {
        opening_balance = labelled_amount(rows, label, mapping, currency)?;
    }
    if let Some(label) = &mapping.closing_balance_label {
        closing_balance = labelled_amount(rows, label, mapping, currency)?;
    }
    if let Some((opening, closing)) = running.as_deref().and_then(balances_from_running) {
        opening_balance = opening_balance.or(Some(opening));
        closing_balance = closing_balance.or(Some(closing));
    }
    Ok(ParsedStatement {
        rows: parsed,
        opening_balance,
        closing_balance,
    })
}

/// The `(amount, balance after)` of each line, for the running balance.
type Running = Vec<(Money, Option<Money>)>;

/// The rows below the header, and the running balances of the lines: `None`
/// when a row that may have moved the balance (it has one) could not be
/// read, since the chain would then skip it without anyone noticing.
fn read_body(
    rows: &[Vec<Cell>],
    header_index: usize,
    columns: &Columns,
    mapping: &Mapping,
    currency: &Currency,
) -> (Vec<ParsedRow>, Option<Running>) {
    let mut parsed = Vec::new();
    let mut running = Some(Vec::new());
    for (index, row) in rows.iter().enumerate().skip(header_index + 1) {
        if row.iter().all(Cell::is_empty) {
            continue;
        }
        let number = index + 1;
        match read_line(number, row, columns, mapping, currency) {
            Ok(line) => {
                if let (Some(at), Some(chain)) = (columns.running_balance, running.as_mut()) {
                    chain.push((line.amount, running_balance(row, at, mapping, currency)));
                }
                parsed.push(ParsedRow::Line(line));
            }
            Err(reason) => {
                let unreadable = matches!(
                    reason,
                    Unsupported::UnreadableAmount(_) | Unsupported::UnreadableDate(_)
                );
                if unreadable
                    && columns
                        .running_balance
                        .is_some_and(|at| !cell(row, at).is_empty())
                {
                    running = None;
                }
                parsed.push(ParsedRow::Unsupported {
                    row: number,
                    reason,
                });
            }
        }
    }
    (parsed, running)
}

/// The position of the first of `names` in the header row (L5).
pub(crate) fn find(header: &[Cell], names: &ColumnNames) -> Option<usize> {
    names.iter().find_map(|name| {
        header
            .iter()
            .position(|cell| cell.text().eq_ignore_ascii_case(name.trim()))
    })
}

pub(crate) fn require(header: &[Cell], names: &ColumnNames) -> Result<usize, ParseError> {
    find(header, names).ok_or_else(|| ParseError::MissingColumn(names.clone()))
}

/// A configured optional column must exist too: a typo in a profile should
/// fail loudly, not import lines without their IDs.
fn optional(header: &[Cell], names: &Option<ColumnNames>) -> Result<Option<usize>, ParseError> {
    names
        .as_ref()
        .map(|names| require(header, names))
        .transpose()
}

fn resolve(header: &[Cell], mapping: &Mapping) -> Result<Columns, ParseError> {
    let amount = match &mapping.amount {
        AmountColumns::Signed { column } => AmountAt::Signed(require(header, column)?),
        AmountColumns::Split { credit, debit } => AmountAt::Split {
            credit: require(header, credit)?,
            debit: require(header, debit)?,
        },
    };
    let skip = mapping
        .skip_when
        .iter()
        .map(|rule| {
            Ok((
                require(header, &rule.column)?,
                rule.contains.to_lowercase(),
                rule.reason.clone(),
            ))
        })
        .collect::<Result<_, ParseError>>()?;
    Ok(Columns {
        date: require(header, &mapping.date_column)?,
        value_date: optional(header, &mapping.value_date_column)?,
        amount,
        description: mapping
            .description_columns
            .iter()
            .map(|names| require(header, names))
            .collect::<Result<_, _>>()?,
        category: optional(header, &mapping.category_column)?,
        counterparty: optional(header, &mapping.counterparty_column)?,
        counterparty_iban: optional(header, &mapping.counterparty_iban_column)?,
        external_id: optional(header, &mapping.external_id_column)?,
        currency: optional(header, &mapping.currency_column)?,
        // Voucher columns come and go between exports: a missing one is fine.
        notes: present(header, &mapping.note_columns),
        parts: parts_present(header, &mapping.part_columns)?,
        running_balance: optional(header, &mapping.running_balance_column)?,
        skip,
    })
}

pub(crate) fn cell(row: &[Cell], at: usize) -> &Cell {
    row.get(at).unwrap_or(&Cell::Empty)
}

fn text(row: &[Cell], at: Option<usize>) -> Option<String> {
    let value = cell(row, at?).text();
    (!value.is_empty()).then_some(value)
}

/// One row into a line, or the reason it is not one.
fn read_line(
    number: usize,
    row: &[Cell],
    columns: &Columns,
    mapping: &Mapping,
    currency: &Currency,
) -> Result<StatementLine, Unsupported> {
    let accounting_date = date(cell(row, columns.date), &mapping.date_formats)
        .map_err(Unsupported::UnreadableDate)?
        .ok_or(Unsupported::NotATransaction)?;
    let value_date = match columns.value_date {
        Some(at) => date(cell(row, at), &mapping.date_formats)
            .map_err(Unsupported::UnreadableDate)?
            .unwrap_or(accounting_date),
        None => accounting_date,
    };
    if let Some(code) = text(row, columns.currency)
        && !code.eq_ignore_ascii_case(&currency.code().to_string())
    {
        return Err(Unsupported::OtherCurrency(code));
    }
    let description = describe(row, &columns.description);
    check_rules(row, &description, columns, mapping)?;
    let amount = line_amount(row, &columns.amount, mapping.decimal_separator, currency)?;
    let parts = line_parts(row, &columns.parts, mapping.decimal_separator, currency)?;
    if amount.minor() == 0 && parts.is_empty() {
        return Err(Unsupported::NothingMoved);
    }
    Ok(StatementLine {
        row: number,
        accounting_date,
        value_date,
        booked_date: booked(accounting_date, &description, mapping),
        amount,
        category: text(row, columns.category),
        counterparty: text(row, columns.counterparty),
        counterparty_iban: text(row, columns.counterparty_iban),
        external_id: text(row, columns.external_id),
        notes: columns
            .notes
            .iter()
            .filter_map(|(header, at)| {
                text(row, Some(*at)).map(|value| format!("{header}: {value}"))
            })
            .collect(),
        parts,
        description,
    })
}

/// The columns of `lists` the header has, as `(header text, position)`.
fn present(header: &[Cell], lists: &[ColumnNames]) -> Vec<(String, usize)> {
    lists
        .iter()
        .filter_map(|names| find(header, names).map(|at| (header[at].text(), at)))
        .collect()
}

/// The part columns the header has. Some may be missing (exports drop an
/// unused voucher column), but not all: those amounts are money that would
/// silently go missing from the import.
fn parts_present(
    header: &[Cell],
    lists: &[ColumnNames],
) -> Result<Vec<(String, usize)>, ParseError> {
    let found = present(header, lists);
    match lists.first() {
        Some(first) if found.is_empty() => Err(ParseError::MissingColumn(first.clone())),
        _ => Ok(found),
    }
}

/// The non-zero amounts in the part columns (Satispay's vouchers).
fn line_parts(
    row: &[Cell],
    parts: &[(String, usize)],
    separator: char,
    currency: &Currency,
) -> Result<Vec<LinePart>, Unsupported> {
    let mut found = Vec::new();
    for (column, at) in parts {
        let read =
            amount(cell(row, *at), separator, currency).map_err(Unsupported::UnreadableAmount)?;
        if let Some(money) = read.filter(|money| money.minor() != 0) {
            found.push(LinePart {
                column: column.clone(),
                amount: money,
            });
        }
    }
    Ok(found)
}

fn describe(row: &[Cell], columns: &[usize]) -> String {
    columns
        .iter()
        .filter_map(|at| text(row, Some(*at)))
        .collect::<Vec<_>>()
        .join(" - ")
}

/// Pending lines (decision 12) and skip rules.
fn check_rules(
    row: &[Cell],
    description: &str,
    columns: &Columns,
    mapping: &Mapping,
) -> Result<(), Unsupported> {
    let lowered = description.to_lowercase();
    if mapping
        .pending_when
        .iter()
        .any(|marker| lowered.contains(&marker.to_lowercase()))
    {
        return Err(Unsupported::Pending);
    }
    for (at, contains, reason) in &columns.skip {
        if cell(row, *at)
            .text()
            .to_lowercase()
            .contains(contains.as_str())
        {
            return Err(Unsupported::Skipped(reason.clone()));
        }
    }
    Ok(())
}

/// The signed amount; a debit column is always money out (L4).
fn line_amount(
    row: &[Cell],
    at: &AmountAt,
    separator: char,
    currency: &Currency,
) -> Result<Money, Unsupported> {
    let read = |at: usize| {
        amount(cell(row, at), separator, currency).map_err(Unsupported::UnreadableAmount)
    };
    let signed = match at {
        AmountAt::Signed(at) => read(*at)?,
        AmountAt::Split { credit, debit } => {
            let nonzero = |money: Option<Money>| money.filter(|m| m.minor() != 0);
            match (nonzero(read(*credit)?), nonzero(read(*debit)?)) {
                (Some(credit), None) => Some(Money::new(
                    credit.minor().saturating_abs(),
                    credit.currency(),
                )),
                (None, Some(debit)) => Some(Money::new(
                    -debit.minor().saturating_abs(),
                    debit.currency(),
                )),
                (None, None) => None,
                // Money in and out on one line: no way to know which is meant.
                (Some(_), Some(_)) => {
                    return Err(Unsupported::UnreadableAmount(format!(
                        "{} / {}",
                        cell(row, *credit).text(),
                        cell(row, *debit).text()
                    )));
                }
            }
        }
    };
    // Zero when the column is empty: the caller decides whether the parts
    // make it a line.
    Ok(signed.unwrap_or(Money::zero(currency.code())))
}

/// The accounting date moved by the first matching date rule (L8).
fn booked(accounting_date: NaiveDate, description: &str, mapping: &Mapping) -> NaiveDate {
    let lowered = description.to_lowercase();
    let rule = mapping
        .date_rules
        .iter()
        .find(|rule| lowered.contains(&rule.contains.to_lowercase()));
    match rule.map(|rule| rule.book_on) {
        Some(BookOn::PreviousMonthEnd) => accounting_date
            .with_day(1)
            .and_then(|first| first.pred_opt())
            .unwrap_or(accounting_date),
        None => accounting_date,
    }
}

fn running_balance(
    row: &[Cell],
    at: usize,
    mapping: &Mapping,
    currency: &Currency,
) -> Option<Money> {
    amount(cell(row, at), mapping.decimal_separator, currency)
        .ok()
        .flatten()
}

/// Opening and closing balances from `(amount, balance after)` of every
/// line, in file order. Exports list lines newest first or oldest first;
/// the balances themselves tell which: newest first, each balance is the
/// next one plus its own amount; oldest first, the previous one plus it.
/// When a balance is missing or the chain holds neither way, there is no
/// balance rather than a guess.
fn balances_from_running(running: &[(Money, Option<Money>)]) -> Option<(Money, Money)> {
    let chain: Vec<(Money, Money)> = running
        .iter()
        .map(|(amount, balance)| balance.map(|balance| (*amount, balance)))
        .collect::<Option<_>>()?;
    let (first, last) = (chain.first()?, chain.last()?);
    let follows = |earlier: &(Money, Money), later: &(Money, Money)| {
        earlier.1.checked_add(later.0).ok() == Some(later.1)
    };
    let newest_first = chain.windows(2).all(|pair| follows(&pair[1], &pair[0]));
    let oldest_first = chain.windows(2).all(|pair| follows(&pair[0], &pair[1]));
    let (newest, oldest) = match (newest_first, oldest_first) {
        // One line reads the same both ways: its balance after is the closing.
        (true, true) if chain.len() == 1 => (first, last),
        (true, false) => (first, last),
        (false, true) => (last, first),
        // Lines that moved nothing (voucher payments) leave the balance as
        // it was, which reads both ways and is still certain.
        (true, true) if chain.iter().all(|(amount, _)| amount.minor() == 0) => (first, last),
        // Both ways at once (+5 then -5) or neither: no guess.
        (true, true) | (false, false) => return None,
    };
    let opening = oldest.1.checked_sub(oldest.0).ok()?;
    Some((opening, newest.1))
}

/// The amount next to a label anywhere in the sheet (L1): the first
/// non-empty cell to its right. No label, nothing next to it, or text that
/// is not an amount (Widiba puts a second column of labels further right)
/// means no balance; only a number that does not fit the currency is an
/// error.
fn labelled_amount(
    rows: &[Vec<Cell>],
    label: &str,
    mapping: &Mapping,
    currency: &Currency,
) -> Result<Option<Money>, ParseError> {
    for row in rows {
        let Some(at) = row
            .iter()
            .position(|c| c.text().eq_ignore_ascii_case(label.trim()))
        else {
            continue;
        };
        let Some(value) = row[at + 1..].iter().find(|c| !c.is_empty()) else {
            return Ok(None);
        };
        return match (value, amount(value, mapping.decimal_separator, currency)) {
            (_, Ok(balance)) => Ok(balance),
            (Cell::Number(_), Err(value)) => Err(ParseError::BadBalance {
                label: label.to_string(),
                value,
            }),
            (_, Err(_)) => Ok(None),
        };
    }
    Ok(None)
}
