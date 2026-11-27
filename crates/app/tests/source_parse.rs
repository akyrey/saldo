#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

//! Amazon and PayPal export parsers against the owner's anonymised models
//! (Phase 13, Track C).

use chrono::{NaiveDate, NaiveTime};
use chrono_tz::Europe::Rome;
use saldo_app::sources::parse::{
    AmazonRecord, PayPalRecord, SourceRow, parse_amazon, parse_paypal,
};
use saldo_app::statements::parse::{Format, ParseError};
use saldo_core::matching::PayPalKind;
use saldo_core::{Currency, CurrencyCode, Money};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/statements");

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}/{name}")).unwrap()
}

fn currencies() -> Vec<Currency> {
    ["EUR", "USD"]
        .iter()
        .map(|code| Currency::new(CurrencyCode::new(code).unwrap(), 2).unwrap())
        .collect()
}

fn eur(minor: i64) -> Money {
    Money::new(minor, CurrencyCode::new("EUR").unwrap())
}

fn records<T: Clone>(rows: &[SourceRow<T>]) -> Vec<T> {
    rows.iter()
        .filter_map(|row| match row {
            SourceRow::Record(record) => Some(record.clone()),
            SourceRow::Unsupported { .. } => None,
        })
        .collect()
}

#[test]
fn paypal_xlsx_with_a_serial_date_a_time_fraction_and_italian_text_amounts() {
    let rows = parse_paypal(Format::Xlsx, &fixture("paypal.xlsx"), &currencies()).unwrap();
    let found: Vec<PayPalRecord> = records(&rows);
    assert_eq!(found.len(), 1);
    let record = &found[0];
    assert_eq!(record.row, 2);
    assert_eq!(record.date, NaiveDate::from_ymd_opt(2025, 2, 1).unwrap());
    assert_eq!(record.time, NaiveTime::from_hms_opt(21, 25, 42));
    assert_eq!(record.name, "QUATTROZAMPE DI SANTONI LEONARDO");
    assert_eq!(record.kind, PayPalKind::Payment);
    assert_eq!(record.status, "Completata");
    assert_eq!(record.gross, eur(-20_437));
    assert_eq!(record.net, Some(eur(-20_437)));
    assert_eq!(record.transaction_code, "1246914761AFHA");
}

#[test]
fn paypal_csv_with_the_older_column_name_and_two_digit_years() {
    let csv = "\u{feff}\"Data\",\"Orario\",\"Fuso orario\",\"Nome\",\"Tipo\",\"Stato\",\"Valuta\",\"Lordo\",\"Tariffe\",\"Totale\",\"Codice transazione\"\n\
               \"1/2/25\",\"10:00:00\",\"CET\",\"\",\"Bonifico bancario sul conto PayPal\",\"Completata\",\"EUR\",\"12,99\",\"0,00\",\"12,99\",\"AAA1\"\n\
               \"1/2/25\",\"10:00:00\",\"CET\",\"Shop\",\"Pagamento\",\"Completata\",\"JPY\",\"-1.000\",\"0\",\"-1.000\",\"AAA2\"\n\
               \"x\",\"\",\"\",\"\",\"\",\"\",\"EUR\",\"1,00\",\"\",\"\",\"AAA3\"\n";
    let rows = parse_paypal(Format::Csv, csv.as_bytes(), &currencies()).unwrap();
    let found = records(&rows);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind, PayPalKind::Funding);
    assert_eq!(found[0].date, NaiveDate::from_ymd_opt(2025, 2, 1).unwrap());
    assert_eq!(found[0].gross, eur(1_299));
    let reasons: Vec<(usize, String)> = rows
        .iter()
        .filter_map(|row| match row {
            SourceRow::Unsupported { row, reason } => Some((*row, reason.clone())),
            SourceRow::Record(_) => None,
        })
        .collect();
    assert_eq!(
        reasons,
        vec![
            (3, "unknown currency JPY".to_string()),
            (4, "unreadable date \"x\"".to_string()),
        ]
    );
}

#[test]
fn paypal_without_a_transaction_code_column_is_refused() {
    let csv = "Data,Importo\n1/2/25,1\n";
    let error = parse_paypal(Format::Csv, csv.as_bytes(), &currencies()).unwrap_err();
    assert!(matches!(error, ParseError::MissingColumn(_)), "{error:?}");
}

#[test]
fn amazon_csv_items_in_the_household_time_zone() {
    let rows = parse_amazon(&fixture("amazon.csv"), &currencies(), Rome).unwrap();
    let found: Vec<AmazonRecord> = records(&rows);
    assert_eq!(found.len(), 2);
    let first = &found[0];
    assert_eq!(first.order_id, "171-7425934-8821916");
    assert_eq!(first.external_id, "171-7425934-8821916:B00U77VMB2:1");
    assert_eq!(first.date, NaiveDate::from_ymd_opt(2020, 1, 26).unwrap());
    assert_eq!(first.amount, eur(10_389));
    assert!(first.product.starts_with("Bialetti Mokona"));
    assert_eq!(first.payment_method, "MasterCard - 1234");
    assert_eq!(first.status, "Closed");
    assert!(!first.cancelled);
    assert!(!first.gift_card_only);
    assert_eq!(found[1].amount, eur(500));
}

#[test]
fn amazon_late_evening_utc_is_the_next_day_in_rome_and_cancelled_items_are_marked() {
    let csv = "Order ID,Order Date,Currency,Total Amount,Product Name,Order Status,Payment Method Type,ASIN\n\
               405-1234567-1234567,2024-01-05T23:30:00Z,EUR,12.34,Book,Cancelled,Visa - 1111,B1\n\
               405-1234567-1234567,2024-01-05T23:30:00Z,EUR,1.00,Card,Closed,Gift Certificate/Card,B2\n\
               405-1234567-1234567,2024-01-05T23:30:00Z,EUR,2.00,Pen,Closed,Visa - 1111,B2\n\
               406-1234567-1234567,Not Available,EUR,1.00,Lost,Closed,Visa - 1111,B3\n";
    let rows = parse_amazon(csv.as_bytes(), &currencies(), Rome).unwrap();
    let found = records(&rows);
    assert_eq!(found[0].date, NaiveDate::from_ymd_opt(2024, 1, 6).unwrap());
    assert!(found[0].cancelled);
    assert!(found[1].gift_card_only);
    // The same product twice in one order keeps distinct IDs.
    assert_eq!(found[2].external_id, "405-1234567-1234567:B2:2");
    assert_eq!(found.len(), 3);
    assert!(matches!(
        rows.last(),
        Some(SourceRow::Unsupported { row: 5, .. })
    ));
}
