#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

//! Amazon and PayPal exports stored per household (Phase 13, Track C).

mod common;

use common::*;
use saldo_app::ctx::Origin;
use saldo_app::error::AppError;
use saldo_app::page::PageRequest;
use saldo_app::sources::{RowFilter, SourceUpload};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/statements");

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}/{name}")).unwrap()
}

fn upload(kind: &str, filename: &str, bytes: Vec<u8>) -> SourceUpload {
    SourceUpload {
        kind: kind.to_string(),
        filename: filename.to_string(),
        bytes,
    }
}

/// A household whose app keeps files in its own temporary folder.
async fn household(name: &str) -> (TestApp, saldo_app::ctx::HouseholdCtx) {
    let (mut test, setup) = with_household().await;
    let dir = std::env::temp_dir().join(format!("saldo-sources-{name}-{}", std::process::id()));
    test.app = test.app.with_data_dir(dir);
    (test, setup.ctx)
}

const AMAZON_HEADER: &str = "Order ID,Order Date,Currency,Total Amount,Product Name,Order Status,Payment Method Type,ASIN\n";

#[tokio::test]
async fn an_amazon_export_is_stored_with_its_rows_and_an_event() {
    let (test, ctx) = household("stored").await;
    let sources = &test.app.sources;
    let document = sources
        .upload(
            &ctx,
            Origin::Api,
            upload("amazon_orders", "orders.csv", fixture("amazon.csv")),
        )
        .await
        .unwrap();
    assert_eq!(document.kind, "amazon_orders");
    assert_eq!(
        (
            document.rows_added,
            document.rows_updated,
            document.row_count
        ),
        (2, 0, 2)
    );
    assert_eq!(document.first_date.as_deref(), Some("2020-01-26"));
    assert_eq!(document.last_date.as_deref(), Some("2022-11-30"));
    assert_eq!(
        last_event(&test, &ctx).await.event_type,
        "source_document.created"
    );

    let rows = sources
        .rows(&ctx, RowFilter::default(), PageRequest::default())
        .await
        .unwrap();
    assert_eq!(rows.total, 2);
    let first = &rows.items[0];
    assert_eq!(first.group_key.as_deref(), Some("171-7425934-8821916"));
    assert_eq!(first.amount, "103.89");
    assert_eq!(first.row_kind, "item");
    assert!(!first.matched);
    assert_eq!(sources.documents(&ctx).await.unwrap().len(), 1);
}

#[tokio::test]
async fn the_same_file_twice_is_a_conflict_and_a_newer_export_adds_only_new_rows() {
    let (test, ctx) = household("conflict").await;
    let sources = &test.app.sources;
    let first = format!(
        "{AMAZON_HEADER}405-1-1,2024-01-05T10:00:00Z,EUR,12.34,Book,Shipping,Visa - 1,B1\n"
    );
    sources
        .upload(
            &ctx,
            Origin::Api,
            upload("amazon_orders", "a.csv", first.clone().into_bytes()),
        )
        .await
        .unwrap();
    let again = sources
        .upload(
            &ctx,
            Origin::Api,
            upload("amazon_orders", "a.csv", first.into_bytes()),
        )
        .await;
    assert!(matches!(again, Err(AppError::Conflict(_))), "{again:?}");

    let newer = format!(
        "{AMAZON_HEADER}405-1-1,2024-01-05T10:00:00Z,EUR,12.34,Book,Closed,Visa - 1,B1\n\
         406-1-1,2024-02-01T10:00:00Z,EUR,5.00,Pen,Closed,Visa - 1,B2\n"
    );
    let document = sources
        .upload(
            &ctx,
            Origin::Api,
            upload("amazon_orders", "b.csv", newer.into_bytes()),
        )
        .await
        .unwrap();
    assert_eq!((document.rows_added, document.rows_updated), (1, 1));
    let rows = sources
        .rows(&ctx, RowFilter::default(), PageRequest::default())
        .await
        .unwrap();
    assert_eq!(rows.total, 2);
    assert_eq!(rows.items[0].status.as_deref(), Some("Closed"));
}

#[tokio::test]
async fn a_paypal_export_classifies_its_rows() {
    let (test, ctx) = household("paypal").await;
    let sources = &test.app.sources;
    sources
        .upload(
            &ctx,
            Origin::Api,
            upload("paypal", "paypal.csv", fixture("paypal.csv")),
        )
        .await
        .unwrap();
    let filter = RowFilter {
        kind: Some("paypal".to_string()),
        ..RowFilter::default()
    };
    let rows = sources
        .rows(&ctx, filter, PageRequest::default())
        .await
        .unwrap();
    let kinds: Vec<(&str, &str)> = rows
        .items
        .iter()
        .map(|r| (r.row_kind.as_str(), r.amount.as_str()))
        .collect();
    assert_eq!(kinds, vec![("payment", "-204.37"), ("funding", "204.37")]);
    assert_eq!(rows.items[0].time.as_deref(), Some("21:25:42"));
    assert_eq!(
        rows.items[0].counterparty.as_deref(),
        Some("QUATTROZAMPE DI SANTONI LEONARDO")
    );
}

#[tokio::test]
async fn rows_can_be_filtered_by_date_and_kind() {
    let (test, ctx) = household("filter").await;
    let sources = &test.app.sources;
    sources
        .upload(
            &ctx,
            Origin::Api,
            upload("amazon_orders", "orders.csv", fixture("amazon.csv")),
        )
        .await
        .unwrap();
    sources
        .upload(
            &ctx,
            Origin::Api,
            upload("paypal", "paypal.csv", fixture("paypal.csv")),
        )
        .await
        .unwrap();
    let filter = RowFilter {
        from: chrono::NaiveDate::from_ymd_opt(2022, 1, 1),
        to: chrono::NaiveDate::from_ymd_opt(2022, 12, 31),
        ..RowFilter::default()
    };
    let rows = sources
        .rows(&ctx, filter, PageRequest::default())
        .await
        .unwrap();
    assert_eq!(rows.total, 1);
    assert_eq!(rows.items[0].kind, "amazon_orders");
}

#[tokio::test]
async fn deleting_a_document_removes_only_the_rows_no_other_document_holds() {
    let (test, ctx) = household("delete").await;
    let sources = &test.app.sources;
    let older =
        format!("{AMAZON_HEADER}405-1-1,2024-01-05T10:00:00Z,EUR,12.34,Book,Closed,Visa - 1,B1\n");
    let newer = format!(
        "{AMAZON_HEADER}405-1-1,2024-01-05T10:00:00Z,EUR,12.34,Book,Closed,Visa - 1,B1\n\
         406-1-1,2024-02-01T10:00:00Z,EUR,5.00,Pen,Closed,Visa - 1,B2\n"
    );
    let first = sources
        .upload(
            &ctx,
            Origin::Api,
            upload("amazon_orders", "a.csv", older.into_bytes()),
        )
        .await
        .unwrap();
    let second = sources
        .upload(
            &ctx,
            Origin::Api,
            upload("amazon_orders", "b.csv", newer.into_bytes()),
        )
        .await
        .unwrap();
    // The newer export holds both rows: deleting it removes only the pen.
    sources
        .delete_document(&ctx, Origin::Api, &second.id)
        .await
        .unwrap();
    let rows = sources
        .rows(&ctx, RowFilter::default(), PageRequest::default())
        .await
        .unwrap();
    assert_eq!(rows.total, 1);
    assert_eq!(rows.items[0].text.as_deref(), Some("Book"));
    assert_eq!(
        last_event(&test, &ctx).await.event_type,
        "source_document.deleted"
    );
    sources
        .delete_document(&ctx, Origin::Api, &first.id)
        .await
        .unwrap();
    assert_eq!(
        sources
            .rows(&ctx, RowFilter::default(), PageRequest::default())
            .await
            .unwrap()
            .total,
        0
    );
    assert!(sources.documents(&ctx).await.unwrap().is_empty());
}

#[tokio::test]
async fn bad_uploads_and_other_households_are_refused() {
    let (test, ctx) = household("bad").await;
    let sources = &test.app.sources;
    let kind = sources
        .upload(&ctx, Origin::Api, upload("ebay", "x.csv", b"a".to_vec()))
        .await;
    assert!(
        matches!(&kind, Err(AppError::Invalid { field, .. }) if field == "kind"),
        "{kind:?}"
    );
    let file = sources
        .upload(
            &ctx,
            Origin::Api,
            upload("paypal", "x.csv", b"nothing,here\n".to_vec()),
        )
        .await;
    assert!(
        matches!(&file, Err(AppError::Invalid { field, .. }) if field == "file"),
        "{file:?}"
    );

    let document = sources
        .upload(
            &ctx,
            Origin::Api,
            upload("amazon_orders", "orders.csv", fixture("amazon.csv")),
        )
        .await
        .unwrap();
    let other = other_household(&test).await;
    assert!(sources.documents(&other).await.unwrap().is_empty());
    assert_eq!(
        sources
            .rows(&other, RowFilter::default(), PageRequest::default())
            .await
            .unwrap()
            .total,
        0
    );
    let deleted = sources
        .delete_document(&other, Origin::Api, &document.id)
        .await;
    assert!(matches!(deleted, Err(AppError::NotFound)));
}
