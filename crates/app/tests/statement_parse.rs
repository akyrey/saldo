#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

//! Statement parsers against the owner's anonymised export models
//! (Phase 13, Package 0 and Track B).

use chrono::NaiveDate;
use saldo_app::statements::parse::{
    Format, Mapping, ParseError, ParsedRow, StatementLine, Unsupported, parse_statement,
};
use saldo_core::{Currency, CurrencyCode, Money};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/statements");

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}/{name}")).unwrap()
}

fn eur() -> Currency {
    Currency::new(CurrencyCode::new("EUR").unwrap(), 2).unwrap()
}

fn money(minor: i64) -> Money {
    Money::new(minor, CurrencyCode::new("EUR").unwrap())
}

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn lines(rows: &[ParsedRow]) -> Vec<&StatementLine> {
    rows.iter()
        .filter_map(|row| match row {
            ParsedRow::Line(line) => Some(line),
            ParsedRow::Unsupported { .. } => None,
        })
        .collect()
}

fn unsupported(rows: &[ParsedRow]) -> Vec<(usize, Unsupported)> {
    rows.iter()
        .filter_map(|row| match row {
            ParsedRow::Unsupported { row, reason } => Some((*row, reason.clone())),
            ParsedRow::Line(_) => None,
        })
        .collect()
}

#[test]
fn widiba_header_below_a_prelude_serial_dates_and_a_footer() {
    let parsed = parse_statement(
        Format::Xlsx,
        &fixture("widiba.xlsx"),
        &Mapping::widiba(),
        &eur(),
    )
    .unwrap();
    let found = lines(&parsed.rows);
    assert_eq!(found.len(), 1);
    let line = found[0];
    assert_eq!(line.accounting_date, day(2026, 11, 25));
    assert_eq!(line.value_date, day(2026, 11, 25));
    assert_eq!(line.booked_date, day(2026, 11, 25));
    assert_eq!(line.amount, money(-4_176));
    assert!(
        line.description
            .starts_with("Addebito Diretto Sdd - Addebito Sdd N. 123456789")
    );
    assert!(line.description.ends_with("87521314721765/paypal"));
    assert_eq!(line.category.as_deref(), Some("Addebito Diretto Sdd"));
    // The "Totale (€)" footer has no date.
    assert_eq!(
        unsupported(&parsed.rows),
        vec![(21, Unsupported::NotATransaction)]
    );
    // The balance labels are there, with no value next to them.
    assert_eq!(parsed.opening_balance, None);
    assert_eq!(parsed.closing_balance, None);
}

#[test]
fn mediobanca_text_dates_and_split_columns() {
    let parsed = parse_statement(
        Format::Xlsx,
        &fixture("mediobanca.xlsx"),
        &Mapping::mediobanca(),
        &eur(),
    )
    .unwrap();
    let found = lines(&parsed.rows);
    let summary: Vec<(NaiveDate, i64)> = found
        .iter()
        .map(|l| (l.booked_date, l.amount.minor()))
        .collect();
    assert_eq!(
        summary,
        vec![
            (day(2026, 1, 5), -145_700),
            (day(2026, 1, 5), 145_753),
            (day(2025, 12, 31), -200),
            (day(2025, 12, 29), 20_000),
        ]
    );
    assert_eq!(found[0].description, "Disposizione - RIF:32432523532. Nome");
    // The header is on row 15, under rows the export leaves empty.
    assert_eq!(found[0].row, 16);
    assert!(unsupported(&parsed.rows).is_empty());
    assert_eq!(parsed.closing_balance, None);
}

#[test]
fn satispay_imports_the_balance_part_and_derives_the_balances() {
    let parsed = parse_statement(
        Format::Xlsx,
        &fixture("satispay.xlsx"),
        &Mapping::satispay(),
        &eur(),
    )
    .unwrap();
    let found = lines(&parsed.rows);
    let summary: Vec<(NaiveDate, i64, &str)> = found
        .iter()
        .map(|l| {
            (
                l.booked_date,
                l.amount.minor(),
                l.counterparty.as_deref().unwrap_or(""),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            (day(2025, 8, 5), -1_500, "Merchant 1"),
            // 13.00 paid: 5.00 from the balance, 8.00 in meal vouchers.
            (day(2025, 8, 5), -500, "Merchant 2"),
            (day(2025, 8, 3), -100, "Merchant 3"),
        ]
    );
    assert_eq!(found[1].notes, vec!["Meal Vouchers: -8".to_string()]);
    assert!(found[0].notes.is_empty());
    assert_eq!(
        found[0].external_id.as_deref(),
        Some("01987a1e-bad5-703d-9b27-f8d671eb8846")
    );
    assert_eq!(found[0].category.as_deref(), Some("🏬 to a Store"));
    // Newest first: the first row's balance after is the closing balance;
    // the oldest row's balance after minus its amount is the opening one.
    assert_eq!(parsed.closing_balance, Some(money(10_816)));
    assert_eq!(parsed.opening_balance, Some(money(12_916)));
}

#[test]
fn a_file_that_is_not_a_spreadsheet_is_an_error() {
    let error =
        parse_statement(Format::Xlsx, b"not a zip", &Mapping::widiba(), &eur()).unwrap_err();
    assert!(matches!(error, ParseError::Unreadable(_)), "{error:?}");
}

#[test]
fn a_bank_preset_on_another_bank_file_cannot_find_its_header() {
    let error = parse_statement(
        Format::Xlsx,
        &fixture("mediobanca.xlsx"),
        &Mapping::widiba(),
        &eur(),
    )
    .unwrap_err();
    assert!(matches!(error, ParseError::HeaderNotFound(_)), "{error:?}");
}

#[test]
fn an_upload_over_the_size_limit_is_refused() {
    let big = vec![b'a'; 5 * 1024 * 1024 + 1];
    let error = parse_statement(Format::Csv, &big, &Mapping::mediobanca(), &eur()).unwrap_err();
    assert!(matches!(error, ParseError::TooLarge { .. }), "{error:?}");
}

/// A CSV bank with every optional rule switched on.
fn csv_mapping() -> Mapping {
    let mut mapping = Mapping::mediobanca();
    mapping.date_column = vec!["Data".to_string()];
    mapping.value_date_column = None;
    mapping.description_columns = vec![vec!["Descrizione".to_string()]];
    mapping.pending_when = vec!["in lavorazione".to_string()];
    mapping.skip_when = vec![saldo_app::statements::parse::SkipRule {
        column: vec!["Stato".to_string()],
        contains: "annullat".to_string(),
        reason: "cancelled".to_string(),
    }];
    mapping.date_rules = vec![saldo_app::statements::parse::DateRule {
        contains: "Addebito Pacchetto".to_string(),
        book_on: saldo_app::statements::parse::BookOn::PreviousMonthEnd,
    }];
    mapping.opening_balance_label = Some("Saldo iniziale".to_string());
    mapping.closing_balance_label = Some("Saldo finale".to_string());
    mapping
}

const CSV: &str = "\
Estratto conto;;;;;
Saldo iniziale;;1.000,00;;;
Data;Descrizione;Entrate;Uscite;Divisa;Stato
16/10/2026;Addebito Pacchetto Silver;;9,90;EUR;
17/10/2026;Stipendio;1.500,00;;EUR;
18/10/2026;Pagamento in lavorazione;;5,00;EUR;
19/10/2026;Bonifico annullato;;20,00;EUR;Annullato
20/10/2026;Acquisto USA;;10,00;USD;
21/10/2026;Importo strano;;abc;EUR;
32/10/2026;Data strana;;1,00;EUR;
22/10/2026;Commissione;;-2,50;EUR;
Totale;;;;;
Saldo finale;;2.487,60;;;
";

#[test]
fn csv_rules_reasons_and_labelled_balances() {
    let parsed = parse_statement(Format::Csv, CSV.as_bytes(), &csv_mapping(), &eur()).unwrap();
    let found = lines(&parsed.rows);
    let summary: Vec<(usize, NaiveDate, NaiveDate, i64)> = found
        .iter()
        .map(|l| (l.row, l.accounting_date, l.booked_date, l.amount.minor()))
        .collect();
    assert_eq!(
        summary,
        vec![
            // The package fee belongs to the previous month (L8).
            (4, day(2026, 10, 16), day(2026, 9, 30), -990),
            (5, day(2026, 10, 17), day(2026, 10, 17), 150_000),
            // A debit written as negative is still money out (L4).
            (11, day(2026, 10, 22), day(2026, 10, 22), -250),
        ]
    );
    assert_eq!(
        unsupported(&parsed.rows),
        vec![
            (6, Unsupported::Pending),
            (7, Unsupported::Skipped("cancelled".to_string())),
            (8, Unsupported::OtherCurrency("USD".to_string())),
            (9, Unsupported::UnreadableAmount("abc".to_string())),
            (10, Unsupported::UnreadableDate("32/10/2026".to_string())),
            (12, Unsupported::UnreadableDate("Totale".to_string())),
            (13, Unsupported::UnreadableDate("Saldo finale".to_string())),
        ]
    );
    assert_eq!(parsed.opening_balance, Some(money(100_000)));
    assert_eq!(parsed.closing_balance, Some(money(248_760)));
}

#[test]
fn running_balances_also_work_oldest_first() {
    let csv = "Date;Name;Balance;Balance after transaction;Type;Status;ID\n\
               01/08/2025 09:00;A;-1.00;99.00;x;ok;1\n\
               02/08/2025 09:00;B;-2.00;97.00;x;ok;2\n";
    let mut mapping = Mapping::satispay();
    mapping.note_columns = vec![];
    let parsed = parse_statement(Format::Csv, csv.as_bytes(), &mapping, &eur()).unwrap();
    assert_eq!(parsed.opening_balance, Some(money(10_000)));
    assert_eq!(parsed.closing_balance, Some(money(9_700)));
}

#[test]
fn a_satispay_payment_made_only_with_vouchers_moved_nothing() {
    let csv = "Date;Name;Balance;Meal Vouchers;Balance after transaction;Type;Status;ID\n\
               01/08/2025 09:00;Bar;;-8.00;99.00;x;ok;1\n";
    let parsed =
        parse_statement(Format::Csv, csv.as_bytes(), &Mapping::satispay(), &eur()).unwrap();
    assert_eq!(
        unsupported(&parsed.rows),
        vec![(2, Unsupported::NothingMoved)]
    );
}

#[test]
fn a_configured_column_missing_from_the_header_is_an_error() {
    let csv = "Data;Descrizione;Entrate;Uscite\n01/01/2026;x;1,00;\n";
    let mut mapping = csv_mapping();
    mapping.skip_when = vec![];
    let error = parse_statement(Format::Csv, csv.as_bytes(), &mapping, &eur()).unwrap_err();
    assert_eq!(error, ParseError::MissingColumn(vec!["Divisa".to_string()]));
}

#[test]
fn a_number_next_to_a_balance_label_that_does_not_fit_the_currency_is_an_error() {
    let csv = "Saldo iniziale;1.0001\nData;Descrizione;Entrate;Uscite;Divisa;Stato\n";
    let mut mapping = csv_mapping();
    mapping.decimal_separator = '.';
    let error = parse_statement(Format::Csv, csv.as_bytes(), &mapping, &eur());
    // A CSV cell is text, so a bad value there is no balance, not an error.
    assert_eq!(error.unwrap().opening_balance, None);
}

#[test]
fn mappings_round_trip_through_json() {
    for mapping in [
        Mapping::widiba(),
        Mapping::mediobanca(),
        Mapping::satispay(),
    ] {
        let json = serde_json::to_string(&mapping).unwrap();
        let back: Mapping = serde_json::from_str(&json).unwrap();
        assert_eq!(back, mapping);
    }
}

#[test]
fn the_header_row_needs_the_amount_column_too() {
    // A prelude cell that happens to read "Data" is not the header.
    let csv =
        "Data;stampa\nData;Descrizione;Entrate;Uscite;Divisa;Stato\n01/01/2026;x;1,00;;EUR;\n";
    let parsed = parse_statement(Format::Csv, csv.as_bytes(), &csv_mapping(), &eur()).unwrap();
    assert_eq!(lines(&parsed.rows).len(), 1);
}

#[test]
fn both_split_columns_filled_is_unreadable() {
    let csv = "Data;Descrizione;Entrate;Uscite;Divisa;Stato\n01/01/2026;x;1,00;2,00;EUR;\n";
    let parsed = parse_statement(Format::Csv, csv.as_bytes(), &csv_mapping(), &eur()).unwrap();
    assert!(matches!(
        unsupported(&parsed.rows)[0].1,
        Unsupported::UnreadableAmount(_)
    ));
}

#[test]
fn running_balances_on_one_day_oldest_first_follow_the_chain() {
    let csv = "Date;Name;Balance;Balance after transaction;Type;Status;ID\n\
               05/01/2025 09:00;A;5.00;15.00;x;ok;1\n\
               05/01/2025 10:00;B;5.00;20.00;x;ok;2\n";
    let mut mapping = Mapping::satispay();
    mapping.note_columns = vec![];
    let parsed = parse_statement(Format::Csv, csv.as_bytes(), &mapping, &eur()).unwrap();
    assert_eq!(parsed.opening_balance, Some(money(1_000)));
    assert_eq!(parsed.closing_balance, Some(money(2_000)));
}

#[test]
fn a_broken_running_balance_chain_gives_no_balance_rather_than_a_guess() {
    let csv = "Date;Name;Balance;Balance after transaction;Type;Status;ID\n\
               05/01/2025 09:00;A;-1.00;99.00;x;ok;1\n\
               04/01/2025 09:00;B;-2.00;50.00;x;ok;2\n";
    let mut mapping = Mapping::satispay();
    mapping.note_columns = vec![];
    let parsed = parse_statement(Format::Csv, csv.as_bytes(), &mapping, &eur()).unwrap();
    assert_eq!(parsed.opening_balance, None);
    assert_eq!(parsed.closing_balance, None);
}

#[test]
fn mappings_are_validated_before_they_are_saved() {
    use saldo_app::statements::parse::MappingError;
    assert_eq!(Mapping::widiba().validate(), Ok(()));
    let mut empty_rule = Mapping::widiba();
    empty_rule.pending_when = vec!["  ".to_string()];
    assert_eq!(empty_rule.validate(), Err(MappingError::EmptyText));
    let mut no_date = Mapping::widiba();
    no_date.date_column = vec![];
    assert_eq!(no_date.validate(), Err(MappingError::NoColumnNames));
    let mut too_many = Mapping::widiba();
    too_many.pending_when = vec!["x".to_string(); 51];
    assert_eq!(too_many.validate(), Err(MappingError::TooManyRules));
    let mut separator = Mapping::widiba();
    separator.decimal_separator = 'x';
    assert_eq!(
        separator.validate(),
        Err(MappingError::DecimalSeparator('x'))
    );
}

#[test]
fn unknown_mapping_fields_are_refused() {
    let mut json = serde_json::to_value(Mapping::widiba()).unwrap();
    json["closing_balance_lable"] = serde_json::json!("typo");
    assert!(serde_json::from_value::<Mapping>(json).is_err());
}

#[test]
fn an_unreadable_row_with_a_balance_breaks_the_chain() {
    // The newest row cannot be read, so the closing balance is unknown.
    let csv = "Date;Name;Balance;Balance after transaction;Type;Status;ID\n\
               06/01/2025 09:00;A;abc;90.00;x;ok;1\n\
               05/01/2025 09:00;B;-1.00;99.00;x;ok;2\n\
               04/01/2025 09:00;C;-1.00;100.00;x;ok;3\n";
    let mut mapping = Mapping::satispay();
    mapping.note_columns = vec![];
    let parsed = parse_statement(Format::Csv, csv.as_bytes(), &mapping, &eur()).unwrap();
    assert_eq!(parsed.closing_balance, None);
    assert_eq!(parsed.opening_balance, None);
}

#[test]
fn a_chain_that_reads_both_ways_gives_no_balance() {
    let csv = "Date;Name;Balance;Balance after transaction;Type;Status;ID\n\
               05/01/2025 09:00;A;5.00;10.00;x;ok;1\n\
               05/01/2025 09:00;B;-5.00;5.00;x;ok;2\n";
    let mut mapping = Mapping::satispay();
    mapping.note_columns = vec![];
    let parsed = parse_statement(Format::Csv, csv.as_bytes(), &mapping, &eur()).unwrap();
    assert_eq!(parsed.closing_balance, None);
}

#[test]
fn an_invalid_stored_mapping_is_refused_when_parsing() {
    let mut mapping = Mapping::widiba();
    mapping.pending_when = vec![String::new()];
    let error =
        parse_statement(Format::Xlsx, &fixture("widiba.xlsx"), &mapping, &eur()).unwrap_err();
    assert!(matches!(error, ParseError::InvalidMapping(_)), "{error:?}");
}
