#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

//! Enriching existing transactions from stored Amazon and PayPal rows
//! (Phase 13, Track E).

mod common;

use chrono::NaiveDate;
use common::*;
use saldo_app::accounts::AccountView;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::reconciliations::NewReconciliation;
use saldo_app::sources::SourceUpload;
use saldo_app::statements::EnrichmentAccept;
use saldo_app::transactions::TransactionGroupView;

const ORDER: &str = "405-1234567-1234567";

struct Fixture {
    test: TestApp,
    ctx: HouseholdCtx,
    checking: AccountView,
    shop: AccountView,
}

async fn fixture(name: &str) -> Fixture {
    let (mut test, setup) = with_household().await;
    let dir = std::env::temp_dir().join(format!("saldo-history-{name}-{}", std::process::id()));
    test.app = test.app.with_data_dir(dir);
    let ctx = setup.ctx;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let shop = account(&test, &ctx, "Shop", "expense", None).await;
    Fixture {
        test,
        ctx,
        checking,
        shop,
    }
}

async fn spend(f: &Fixture, date: NaiveDate, amount: &str, text: &str) -> TransactionGroupView {
    let mut s = split(1, &f.checking, &f.shop, amount);
    s.date = date;
    s.description = text.to_string();
    f.test
        .app
        .transactions
        .create(&f.ctx, Origin::Api, group(vec![s]))
        .await
        .unwrap()
}

async fn amazon_order(f: &Fixture) {
    let csv = format!(
        "Order ID,Order Date,Currency,Total Amount,Product Name,Order Status,Payment Method Type,ASIN\n\
         {ORDER},2026-11-02T10:00:00Z,EUR,15.00,Moka pot,Closed,Visa - 1,B1\n\
         {ORDER},2026-11-02T10:00:00Z,EUR,5.00,Filter,Closed,Visa - 1,B2\n"
    );
    let input = SourceUpload {
        kind: "amazon_orders".to_string(),
        filename: "o.csv".to_string(),
        bytes: csv.into_bytes(),
    };
    f.test
        .app
        .sources
        .upload(&f.ctx, Origin::Api, input)
        .await
        .unwrap();
}

fn november() -> (NaiveDate, NaiveDate) {
    (date(1), date(30))
}

#[tokio::test]
async fn a_paypal_payment_in_the_history_gets_its_merchant() {
    let f = fixture("paypal").await;
    let day = NaiveDate::from_ymd_opt(2025, 1, 3).unwrap();
    let old = spend(&f, day, "204.37", "Addebito SDD PayPal Europe").await;
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/statements/paypal.csv"
    ))
    .unwrap();
    let input = SourceUpload {
        kind: "paypal".to_string(),
        filename: "p.csv".to_string(),
        bytes,
    };
    f.test
        .app
        .sources
        .upload(&f.ctx, Origin::Api, input)
        .await
        .unwrap();

    let statements = &f.test.app.statements;
    let (from, to) = (
        NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        NaiveDate::from_ymd_opt(2025, 1, 31).unwrap(),
    );
    let candidates = statements
        .enrichment_candidates(&f.ctx, &f.checking.id, from, to)
        .await
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].split_id, old.splits[0].id);
    assert_eq!(
        candidates[0].proposal["description"],
        "QUATTROZAMPE DI SANTONI LEONARDO"
    );

    let accept = vec![EnrichmentAccept {
        split_id: old.splits[0].id.clone(),
        split_by_item: false,
    }];
    let changed = statements
        .accept_enrichment(&f.ctx, Origin::Api, &f.checking.id, accept)
        .await
        .unwrap();
    let split = &changed[0].splits[0];
    assert_eq!(split.description, "QUATTROZAMPE DI SANTONI LEONARDO");
    let notes = split.notes.clone().unwrap();
    assert!(
        notes.contains("Bank: Addebito SDD PayPal Europe"),
        "{notes}"
    );
    assert!(
        statements
            .enrichment_candidates(&f.ctx, &f.checking.id, from, to)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn an_amazon_purchase_in_the_history_is_split_by_item() {
    let f = fixture("amazon").await;
    amazon_order(&f).await;
    let old = spend(&f, date(5), "20.00", &format!("AMAZON {ORDER}")).await;
    let (from, to) = november();
    let statements = &f.test.app.statements;
    assert_eq!(
        statements
            .enrichment_candidates(&f.ctx, &f.checking.id, from, to)
            .await
            .unwrap()
            .len(),
        1
    );
    let accept = vec![EnrichmentAccept {
        split_id: old.splits[0].id.clone(),
        split_by_item: true,
    }];
    let changed = statements
        .accept_enrichment(&f.ctx, Origin::Api, &f.checking.id, accept)
        .await
        .unwrap();
    let parts: Vec<(&str, &str)> = changed[0]
        .splits
        .iter()
        .map(|s| (s.description.as_str(), s.amount.as_str()))
        .collect();
    assert_eq!(parts, vec![("Moka pot", "15.00"), ("Filter", "5.00")]);
    // The first split keeps its ID (and so its attachments).
    assert_eq!(changed[0].splits[0].id, old.splits[0].id);
}

#[tokio::test]
async fn a_reconciled_transaction_keeps_its_flag_and_is_never_split() {
    let f = fixture("reconciled").await;
    amazon_order(&f).await;
    let old = spend(&f, date(5), "20.00", &format!("AMAZON {ORDER}")).await;
    let reconcile = NewReconciliation {
        statement_date: date(30),
        statement_balance: "-20.00".to_string(),
        split_ids: vec![old.splits[0].id.clone()],
        book_difference: false,
    };
    f.test
        .app
        .reconciliations
        .create(&f.ctx, Origin::Api, account_id(&f.checking), reconcile)
        .await
        .unwrap();
    let statements = &f.test.app.statements;
    let split = vec![EnrichmentAccept {
        split_id: old.splits[0].id.clone(),
        split_by_item: true,
    }];
    let refused = statements
        .accept_enrichment(&f.ctx, Origin::Api, &f.checking.id, split)
        .await;
    assert!(
        matches!(&refused, Err(AppError::Invalid { field, .. }) if field == "split_by_item"),
        "{refused:?}"
    );
    let describe = vec![EnrichmentAccept {
        split_id: old.splits[0].id.clone(),
        split_by_item: false,
    }];
    let changed = statements
        .accept_enrichment(&f.ctx, Origin::Api, &f.checking.id, describe)
        .await
        .unwrap();
    assert!(changed[0].splits[0].reconciled);
    assert_eq!(
        changed[0].splits[0].description,
        "Amazon: Moka pot + 1 more"
    );
}

#[tokio::test]
async fn a_transaction_nothing_explains_is_refused_and_other_households_see_nothing() {
    let f = fixture("none").await;
    let old = spend(&f, date(5), "3.00", "POS BAR").await;
    let statements = &f.test.app.statements;
    let accept = vec![EnrichmentAccept {
        split_id: old.splits[0].id.clone(),
        split_by_item: false,
    }];
    let refused = statements
        .accept_enrichment(&f.ctx, Origin::Api, &f.checking.id, accept)
        .await;
    assert!(matches!(refused, Err(AppError::Conflict(_))), "{refused:?}");

    let other = other_household(&f.test).await;
    let (from, to) = november();
    let theirs = statements
        .enrichment_candidates(&other, &f.checking.id, from, to)
        .await;
    assert!(
        matches!(&theirs, Err(AppError::Invalid { field, .. }) if field == "account_id"),
        "{theirs:?}"
    );
}

#[tokio::test]
async fn a_backwards_or_too_long_range_is_refused() {
    let f = fixture("range").await;
    let statements = &f.test.app.statements;
    let backwards = statements
        .enrichment_candidates(&f.ctx, &f.checking.id, date(30), date(1))
        .await;
    assert!(
        matches!(&backwards, Err(AppError::Invalid { field, .. }) if field == "from"),
        "{backwards:?}"
    );
    let (from, to) = (
        NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
        NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
    );
    let long = statements
        .enrichment_candidates(&f.ctx, &f.checking.id, from, to)
        .await;
    assert!(
        matches!(&long, Err(AppError::Invalid { field, .. }) if field == "to"),
        "{long:?}"
    );
}

#[tokio::test]
async fn too_many_items_at_once_are_refused() {
    let f = fixture("many").await;
    let old = spend(&f, date(5), "3.00", "POS BAR").await;
    let items = (0..201)
        .map(|_| EnrichmentAccept {
            split_id: old.splits[0].id.clone(),
            split_by_item: false,
        })
        .collect();
    let refused = f
        .test
        .app
        .statements
        .accept_enrichment(&f.ctx, Origin::Api, &f.checking.id, items)
        .await;
    assert!(
        matches!(&refused, Err(AppError::Invalid { field, .. }) if field == "items"),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_purchase_with_a_foreign_amount_is_never_split() {
    let f = fixture("foreign").await;
    amazon_order(&f).await;
    let mut s = split(1, &f.checking, &f.shop, "20.00");
    s.date = date(5);
    s.description = format!("AMAZON {ORDER}");
    s.foreign_amount = Some("22.00".to_string());
    s.foreign_currency = Some("USD".to_string());
    let old = f
        .test
        .app
        .transactions
        .create(&f.ctx, Origin::Api, group(vec![s]))
        .await
        .unwrap();
    let split = vec![EnrichmentAccept {
        split_id: old.splits[0].id.clone(),
        split_by_item: true,
    }];
    let refused = f
        .test
        .app
        .statements
        .accept_enrichment(&f.ctx, Origin::Api, &f.checking.id, split)
        .await;
    assert!(
        matches!(&refused, Err(AppError::Invalid { field, .. }) if field == "split_by_item"),
        "{refused:?}"
    );
}
