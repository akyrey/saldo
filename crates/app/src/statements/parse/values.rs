//! Amounts and dates out of cells, without floats (L2, L3).

use chrono::{NaiveDate, NaiveDateTime};
use saldo_core::{Currency, Money};

use super::Cell;

/// The amount in a cell. A number cell holds a plain decimal already (a
/// float artefact such as "0.30000000000000004" is tidied); text uses
/// `decimal_separator`, and the other of `.` and `,` is a thousands
/// separator only between groups of three digits, so a file in the other
/// locale ("5,50" read with `.`) is an error instead of 100 times too big.
/// A currency sign, spaces (also non-breaking ones) and a leading `+` are
/// ignored. Real extra decimals are an error, never rounded. `None` for an
/// empty cell.
pub fn amount(
    cell: &Cell,
    decimal_separator: char,
    currency: &Currency,
) -> Result<Option<Money>, String> {
    match cell {
        Cell::Empty => Ok(None),
        Cell::Text(text) if text.trim().is_empty() => Ok(None),
        Cell::Number(number) => number_amount(number, currency)
            .map(Some)
            .ok_or_else(|| cell.text()),
        Cell::Text(text) => plain_decimal(text, decimal_separator)
            .and_then(|plain| Money::parse_decimal(&plain, currency).ok())
            .map(Some)
            .ok_or_else(|| cell.text()),
        Cell::DateTime(_) => Err(cell.text()),
    }
}

/// How many digits past the currency's decimals must all be 0 or all be 9
/// for a number to count as a float artefact rather than a real amount.
const ARTEFACT_DIGITS: usize = 6;

/// A number cell's text as money. Spreadsheets store binary floats, so a
/// computed 0.1 + 0.2 arrives as "0.30000000000000004": when the digits past
/// the currency's decimals start with six 0s (or six 9s) the value is cut
/// (or rounded away from zero), using only text and integers.
fn number_amount(number: &str, currency: &Currency) -> Option<Money> {
    if let Ok(money) = Money::parse_decimal(number, currency) {
        return Some(money);
    }
    let places = usize::from(currency.decimal_places());
    let (whole, fraction) = number.split_once('.')?;
    let (kept, rest) = (fraction.get(..places)?, fraction.get(places..)?);
    if rest.len() < ARTEFACT_DIGITS || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let cut = if places == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{kept}")
    };
    let money = Money::parse_decimal(&cut, currency).ok()?;
    if rest.starts_with(&"0".repeat(ARTEFACT_DIGITS)) {
        Some(money)
    } else if rest.starts_with(&"9".repeat(ARTEFACT_DIGITS)) {
        let away = if whole.starts_with('-') { -1 } else { 1 };
        Some(Money::new(
            money.minor().checked_add(away)?,
            money.currency(),
        ))
    } else {
        None
    }
}

/// "€ 1.234,56" with `,` as decimal separator becomes "1234.56"; `None`
/// when thousands separators are not between groups of three.
fn plain_decimal(text: &str, decimal_separator: char) -> Option<String> {
    let thousands = if decimal_separator == ',' { '.' } else { ',' };
    let kept: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '€')
        .collect();
    let kept = kept.strip_prefix('+').unwrap_or(&kept);
    let (sign, unsigned) = match kept.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", kept),
    };
    let (whole, fraction) = match unsigned.split_once(decimal_separator) {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (unsigned, None),
    };
    let whole = ungroup(whole, thousands)?;
    Some(match fraction {
        Some(fraction) => format!("{sign}{whole}.{fraction}"),
        None => format!("{sign}{whole}"),
    })
}

/// "1.234.567" with `.` becomes "1234567"; `None` unless the first group
/// has one to three characters and every other group exactly three.
fn ungroup(whole: &str, separator: char) -> Option<String> {
    if !whole.contains(separator) {
        return Some(whole.to_string());
    }
    let groups: Vec<&str> = whole.split(separator).collect();
    let first_fits = groups.first().is_some_and(|g| (1..=3).contains(&g.len()));
    let rest_fit = groups.iter().skip(1).all(|g| g.len() == 3);
    (first_fits && rest_fit).then(|| groups.concat())
}

/// The date in a cell: a spreadsheet date, an Excel serial number (1900
/// system, L2), or text in one of `formats` (a date-time format keeps only
/// its date). `None` for an empty cell.
pub fn date(cell: &Cell, formats: &[String]) -> Result<Option<NaiveDate>, String> {
    match cell {
        Cell::Empty => Ok(None),
        Cell::DateTime(moment) => Ok(Some(moment.date())),
        Cell::Number(serial) => serial_date(serial).map(Some).ok_or_else(|| serial.clone()),
        Cell::Text(text) if text.trim().is_empty() => Ok(None),
        Cell::Text(text) => text_date(text.trim(), formats)
            .map(Some)
            .ok_or_else(|| text.clone()),
    }
}

fn text_date(text: &str, formats: &[String]) -> Option<NaiveDate> {
    formats.iter().find_map(|format| {
        NaiveDate::parse_from_str(text, format).ok().or_else(|| {
            NaiveDateTime::parse_from_str(text, format)
                .ok()
                .map(|moment| moment.date())
        })
    })
}

/// The first day of the 1900 date system, as Excel counts it (it believes
/// 1900 was a leap year, so serials from March 1900 on count from here).
const EXCEL_EPOCH: (i32, u32, u32) = (1899, 12, 30);

/// The date of an Excel serial in the 1900 system: days since 1899-12-30,
/// the fraction being the time of day. Serials before 1900-03-01 (61) are
/// not real statement dates and are refused.
fn serial_date(serial: &str) -> Option<NaiveDate> {
    let whole = serial.split('.').next()?;
    let days: u64 = whole.parse().ok()?;
    if days < 61 {
        return None;
    }
    let (year, month, day) = EXCEL_EPOCH;
    NaiveDate::from_ymd_opt(year, month, day)?.checked_add_days(chrono::Days::new(days))
}

#[cfg(test)]
mod tests {
    use super::*;
    use saldo_core::CurrencyCode;

    fn eur() -> Currency {
        Currency::new(CurrencyCode::new("EUR").unwrap(), 2).unwrap()
    }

    fn minor(cell: Cell, separator: char) -> Result<Option<i64>, String> {
        amount(&cell, separator, &eur()).map(|m| m.map(|m| m.minor()))
    }

    fn text(value: &str) -> Cell {
        Cell::Text(value.to_string())
    }

    fn formats() -> Vec<String> {
        vec![
            "%d/%m/%Y".to_string(),
            "%d/%m/%Y %H:%M".to_string(),
            "%Y-%m-%d".to_string(),
        ]
    }

    #[test]
    fn number_cells_are_plain_decimals() {
        assert_eq!(
            minor(Cell::Number("-41.76".to_string()), ','),
            Ok(Some(-4_176))
        );
        assert_eq!(
            minor(Cell::Number("1457".to_string()), ','),
            Ok(Some(145_700))
        );
        assert_eq!(
            minor(Cell::Number("108.16".to_string()), ','),
            Ok(Some(10_816))
        );
    }

    #[test]
    fn italian_text_amounts() {
        assert_eq!(minor(text("-204,37"), ','), Ok(Some(-20_437)));
        assert_eq!(minor(text("1.234,56"), ','), Ok(Some(123_456)));
        assert_eq!(minor(text(" € 1.234,5 "), ','), Ok(Some(123_450)));
        assert_eq!(minor(text("+12"), ','), Ok(Some(1_200)));
        assert_eq!(minor(text("1\u{a0}234,56"), ','), Ok(Some(123_456)));
    }

    #[test]
    fn dot_decimal_text_amounts() {
        assert_eq!(minor(text("-1,300.00"), '.'), Ok(Some(-130_000)));
        assert_eq!(minor(text("103.89"), '.'), Ok(Some(10_389)));
    }

    #[test]
    fn a_separator_outside_groups_of_three_is_not_a_thousands_separator() {
        // A file in the other locale must not be read 100 times too big.
        assert!(minor(text("5,50"), '.').is_err());
        assert!(minor(text("12.5"), ',').is_err());
        assert!(minor(text("1,23,456.00"), '.').is_err());
        assert_eq!(minor(text("12,345,678.90"), '.'), Ok(Some(1_234_567_890)));
    }

    #[test]
    fn float_artefacts_in_number_cells_are_tidied() {
        assert_eq!(
            minor(Cell::Number("0.30000000000000004".to_string()), ','),
            Ok(Some(30))
        );
        assert_eq!(
            minor(Cell::Number("-41.759999999999998".to_string()), ','),
            Ok(Some(-4_176))
        );
        assert_eq!(
            minor(Cell::Number("9.9999999999".to_string()), ','),
            Ok(Some(1_000))
        );
        // A real third decimal is still an error.
        assert!(minor(Cell::Number("1.005".to_string()), ',').is_err());
    }

    #[test]
    fn unreadable_amounts_are_errors_not_guesses() {
        assert!(minor(text("12,345"), '.').is_ok()); // 12345 with a thousands comma
        assert!(minor(text("1,234"), ',').is_err()); // three decimals in EUR
        assert!(minor(text("abc"), ',').is_err());
        assert!(minor(text("1,2,3"), ',').is_err());
        assert!(minor(text("--5"), ',').is_err());
    }

    #[test]
    fn empty_amounts_are_none() {
        assert_eq!(minor(Cell::Empty, ','), Ok(None));
        assert_eq!(minor(text("  "), ','), Ok(None));
    }

    #[test]
    fn spreadsheet_and_text_dates() {
        let day = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        let moment = day.and_hms_opt(12, 4, 51).unwrap();
        assert_eq!(date(&Cell::DateTime(moment), &formats()), Ok(Some(day)));
        assert_eq!(date(&text("05/01/2026"), &formats()), Ok(Some(day)));
        assert_eq!(date(&text("05/01/2026 12:04"), &formats()), Ok(Some(day)));
        assert_eq!(date(&text("2026-01-05"), &formats()), Ok(Some(day)));
    }

    #[test]
    fn excel_serials_in_number_cells() {
        let expected = NaiveDate::from_ymd_opt(2026, 11, 25).unwrap();
        assert_eq!(
            date(&Cell::Number("46351".to_string()), &formats()),
            Ok(Some(expected))
        );
        let afternoon = NaiveDate::from_ymd_opt(2025, 8, 5).unwrap();
        assert_eq!(
            date(&Cell::Number("45874.503368495367".to_string()), &formats()),
            Ok(Some(afternoon))
        );
    }

    #[test]
    fn unreadable_dates_are_errors_and_empty_ones_none() {
        assert!(date(&text("31/02/2026"), &formats()).is_err());
        assert!(date(&text("Totale"), &formats()).is_err());
        assert!(date(&Cell::Number("-3".to_string()), &formats()).is_err());
        assert_eq!(date(&Cell::Empty, &formats()), Ok(None));
    }
}
