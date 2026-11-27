#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

//! Amazon and PayPal rows explaining statement lines (Phase 13, Track D;
//! brief section 9).

mod common;

use std::collections::BTreeMap;

use common::*;
use saldo_app::accounts::AccountView;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::page::PageRequest;
use saldo_app::sources::{RowFilter, SourceUpload};
use saldo_app::statements::parse::{AmountColumns, Mapping};
use saldo_app::statements::{ImportDetail, LineUpdate, ProfileInput, ProfileView, UploadInput};

const ORDER: &str = "405-1234567-1234567";

struct Fixture {
    test: TestApp,
    ctx: HouseholdCtx,
    checking: AccountView,
    profile: ProfileView,
}

async fn fixture(name: &str) -> Fixture {
    let (mut test, setup) = with_household().await;
    let dir = std::env::temp_dir().join(format!("saldo-enrich-{name}-{}", std::process::id()));
    test.app = test.app.with_data_dir(dir);
    let ctx = setup.ctx;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let names = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let mut mapping = Mapping::mediobanca();
    mapping.date_column = names(&["Data"]);
    mapping.value_date_column = None;
    mapping.amount = AmountColumns::Signed {
        column: names(&["Importo"]),
    };
    mapping.description_columns = vec![names(&["Descrizione"])];
    mapping.currency_column = None;
    let profile = test
        .app
        .statements
        .create_profile(
            &ctx,
            Origin::Api,
            ProfileInput {
                name: "Bank".to_string(),
                account_id: Some(checking.id.clone()),
                format: "csv".to_string(),
                mapping,
                part_accounts: BTreeMap::new(),
                transfer_rules: vec![],
            },
        )
        .await
        .unwrap();
    Fixture {
        test,
        ctx,
        checking,
        profile,
    }
}

async fn source(f: &Fixture, kind: &str, bytes: Vec<u8>) {
    let input = SourceUpload {
        kind: kind.to_string(),
        filename: "export.csv".to_string(),
        bytes,
    };
    f.test
        .app
        .sources
        .upload(&f.ctx, Origin::Api, input)
        .await
        .unwrap();
}

async fn amazon_order(f: &Fixture) {
    let csv = format!(
        "Order ID,Order Date,Currency,Total Amount,Product Name,Order Status,Payment Method Type,ASIN\n\
         {ORDER},2026-11-02T10:00:00Z,EUR,15.00,Moka pot,Closed,Visa - 1,B1\n\
         {ORDER},2026-11-02T10:00:00Z,EUR,5.00,Filter,Closed,Visa - 1,B2\n"
    );
    source(f, "amazon_orders", csv.into_bytes()).await;
}

async fn statement(f: &Fixture, body: &str) -> ImportDetail {
    let input = UploadInput {
        account_id: None,
        profile_id: f.profile.id.clone(),
        filename: "s.csv".to_string(),
        bytes: format!("Data;Descrizione;Importo\n{body}").into_bytes(),
    };
    f.test
        .app
        .statements
        .upload(&f.ctx, Origin::Api, input)
        .await
        .unwrap()
}

async fn commit(f: &Fixture, draft: &ImportDetail) -> ImportDetail {
    f.test
        .app
        .statements
        .commit(&f.ctx, Origin::Api, &draft.import.id)
        .await
        .unwrap()
}

async fn created(
    f: &Fixture,
    detail: &ImportDetail,
    line: usize,
) -> saldo_app::transactions::TransactionGroupView {
    let id = detail.lines[line].created_group_id.clone().unwrap();
    f.test
        .app
        .transactions
        .get(&f.ctx, id.parse().unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn an_amazon_charge_books_the_order_and_keeps_the_bank_text() {
    let f = fixture("amazon").await;
    amazon_order(&f).await;
    let draft = statement(&f, &format!("20/11/2026;AMAZON EU SARL {ORDER};-20,00\n")).await;
    let proposal = draft.lines[0].proposal.clone().unwrap();
    assert_eq!(proposal["description"], "Amazon: Moka pot + 1 more");
    assert!(draft.lines[0].use_proposal);
    let done = commit(&f, &draft).await;
    let group = created(&f, &done, 0).await;
    let split = &group.splits[0];
    assert_eq!(split.description, "Amazon: Moka pot + 1 more");
    let notes = split.notes.clone().unwrap();
    assert!(
        notes.contains(&format!("Bank: AMAZON EU SARL {ORDER}")),
        "{notes}"
    );
    assert!(notes.contains(&format!("Amazon order {ORDER}")), "{notes}");
    let counterparty = f
        .test
        .app
        .accounts
        .get(&f.ctx, split.destination_account_id.parse().unwrap())
        .await
        .unwrap();
    assert_eq!(
        (
            counterparty.name.as_str(),
            counterparty.account_type.as_str()
        ),
        ("Amazon", "expense")
    );
    let rows = f
        .test
        .app
        .sources
        .rows(&f.ctx, RowFilter::default(), PageRequest::default())
        .await
        .unwrap();
    assert!(rows.items.iter().all(|r| r.matched));
}

#[tokio::test]
async fn split_by_item_books_one_split_per_product_adding_up_to_the_charge() {
    let f = fixture("split").await;
    amazon_order(&f).await;
    // Charged 19.00: one euro off (a promotion) comes off the largest item.
    let draft = statement(&f, &format!("20/11/2026;AMAZON {ORDER};-19,00\n")).await;
    let split = LineUpdate {
        split_by_item: Some(true),
        ..LineUpdate::default()
    };
    f.test
        .app
        .statements
        .update_line(&f.ctx, &draft.import.id, 0, split)
        .await
        .unwrap();
    let group = created(&f, &commit(&f, &draft).await, 0).await;
    let parts: Vec<(&str, &str)> = group
        .splits
        .iter()
        .map(|s| (s.description.as_str(), s.amount.as_str()))
        .collect();
    assert_eq!(parts, vec![("Moka pot", "14.00"), ("Filter", "5.00")]);
    assert_eq!(group.title.as_deref(), Some("Amazon: Moka pot + 1 more"));
}

#[tokio::test]
async fn split_by_item_is_refused_without_items() {
    let f = fixture("refuse").await;
    let draft = statement(&f, "20/11/2026;POS BAR;-2,00\n").await;
    let split = LineUpdate {
        split_by_item: Some(true),
        ..LineUpdate::default()
    };
    let result = f
        .test
        .app
        .statements
        .update_line(&f.ctx, &draft.import.id, 0, split)
        .await;
    assert!(
        matches!(&result, Err(AppError::Invalid { field, .. }) if field == "split_by_item"),
        "{result:?}"
    );
}

#[tokio::test]
async fn a_paypal_debit_gets_the_merchant_and_its_existing_account() {
    let f = fixture("paypal").await;
    let merchant = account(
        &f.test,
        &f.ctx,
        "QUATTROZAMPE DI SANTONI LEONARDO",
        "expense",
        None,
    )
    .await;
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/statements/paypal.csv"
    ))
    .unwrap();
    source(&f, "paypal", bytes).await;
    let draft = statement(
        &f,
        "03/01/2025;Addebito SDD PayPal Europe S.a.r.l.;-204,37\n",
    )
    .await;
    assert_eq!(
        draft.lines[0].proposal.clone().unwrap()["counterparty"],
        "QUATTROZAMPE DI SANTONI LEONARDO"
    );
    // Resolved when committing, so turning the proposal off books the bank's view.
    assert_eq!(draft.lines[0].counterparty_account_id, None);
    let group = created(&f, &commit(&f, &draft).await, 0).await;
    assert_eq!(
        group.splits[0].description,
        "QUATTROZAMPE DI SANTONI LEONARDO"
    );
    assert_eq!(group.splits[0].destination_account_id, merchant.id);
}

#[tokio::test]
async fn the_owner_can_keep_the_bank_text_instead() {
    let f = fixture("keep").await;
    amazon_order(&f).await;
    let draft = statement(&f, &format!("20/11/2026;AMAZON {ORDER};-20,00\n")).await;
    let keep = LineUpdate {
        use_proposal: Some(false),
        ..LineUpdate::default()
    };
    f.test
        .app
        .statements
        .update_line(&f.ctx, &draft.import.id, 0, keep)
        .await
        .unwrap();
    let group = created(&f, &commit(&f, &draft).await, 0).await;
    assert_eq!(group.splits[0].description, format!("AMAZON {ORDER}"));
}

#[tokio::test]
async fn an_existing_entry_on_the_order_date_matches_the_charge() {
    let f = fixture("order-date").await;
    amazon_order(&f).await;
    let shop = account(&f.test, &f.ctx, "Amazon", "expense", None).await;
    // Typed by hand on the order day, 18 days before the charge.
    f.test
        .app
        .transactions
        .create(
            &f.ctx,
            Origin::Api,
            group(vec![split(2, &f.checking, &shop, "20.00")]),
        )
        .await
        .unwrap();
    let draft = statement(&f, &format!("20/11/2026;AMAZON {ORDER};-20,00\n")).await;
    assert_eq!(draft.lines[0].status, "possible_match");
}

#[tokio::test]
async fn reverting_frees_the_source_rows_again() {
    let f = fixture("revert").await;
    amazon_order(&f).await;
    let done = commit(
        &f,
        &statement(&f, &format!("20/11/2026;AMAZON {ORDER};-20,00\n")).await,
    )
    .await;
    f.test
        .app
        .statements
        .revert(&f.ctx, Origin::Api, &done.import.id)
        .await
        .unwrap();
    let rows = f
        .test
        .app
        .sources
        .rows(&f.ctx, RowFilter::default(), PageRequest::default())
        .await
        .unwrap();
    assert!(rows.items.iter().all(|r| !r.matched));
}

#[tokio::test]
async fn keeping_the_bank_text_also_keeps_the_banks_counterparty() {
    let f = fixture("bank-view").await;
    let merchant = account(
        &f.test,
        &f.ctx,
        "QUATTROZAMPE DI SANTONI LEONARDO",
        "expense",
        None,
    )
    .await;
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/statements/paypal.csv"
    ))
    .unwrap();
    source(&f, "paypal", bytes).await;
    let draft = statement(&f, "03/01/2025;Addebito SDD PayPal Europe;-204,37\n").await;
    let keep = LineUpdate {
        use_proposal: Some(false),
        ..LineUpdate::default()
    };
    f.test
        .app
        .statements
        .update_line(&f.ctx, &draft.import.id, 0, keep)
        .await
        .unwrap();
    let group = created(&f, &commit(&f, &draft).await, 0).await;
    assert_ne!(group.splits[0].destination_account_id, merchant.id);
    assert_eq!(group.splits[0].description, "Addebito SDD PayPal Europe");
}

#[tokio::test]
async fn a_source_row_explains_one_transaction_only() {
    let f = fixture("once").await;
    amazon_order(&f).await;
    commit(
        &f,
        &statement(&f, &format!("20/11/2026;AMAZON EU {ORDER};-20,00\n")).await,
    )
    .await;
    // Another charge of the same amount days later is not the same order.
    // Eight days after the order: inside the window, so only the booked
    // order being taken keeps it from matching.
    let later = statement(&f, "10/11/2026;AMAZON EU MKTP;-20,00\n").await;
    assert!(later.lines[0].proposal.is_none());
}

#[tokio::test]
async fn skipped_lines_leave_their_source_rows_free() {
    let f = fixture("skip").await;
    amazon_order(&f).await;
    let draft = statement(&f, &format!("20/11/2026;AMAZON {ORDER};-20,00\n")).await;
    let skip = LineUpdate {
        action: Some("skip".to_string()),
        ..LineUpdate::default()
    };
    f.test
        .app
        .statements
        .update_line(&f.ctx, &draft.import.id, 0, skip)
        .await
        .unwrap();
    commit(&f, &draft).await;
    let rows = f
        .test
        .app
        .sources
        .rows(&f.ctx, RowFilter::default(), PageRequest::default())
        .await
        .unwrap();
    assert!(rows.items.iter().all(|r| !r.matched));
}

#[tokio::test]
async fn a_proposal_whose_source_row_is_gone_is_not_booked() {
    let f = fixture("stale").await;
    amazon_order(&f).await;
    let draft = statement(&f, &format!("20/11/2026;AMAZON {ORDER};-20,00\n")).await;
    let documents = f.test.app.sources.documents(&f.ctx).await.unwrap();
    f.test
        .app
        .sources
        .delete_document(&f.ctx, Origin::Api, &documents[0].id)
        .await
        .unwrap();
    let group = created(&f, &commit(&f, &draft).await, 0).await;
    assert_eq!(group.splits[0].description, format!("AMAZON {ORDER}"));
}

#[tokio::test]
async fn a_line_switched_from_skip_to_create_keeps_its_details() {
    let f = fixture("unskip").await;
    amazon_order(&f).await;
    let draft = statement(&f, &format!("20/11/2026;AMAZON {ORDER};-20,00\n")).await;
    for action in ["skip", "create"] {
        let update = LineUpdate {
            action: Some(action.to_string()),
            ..LineUpdate::default()
        };
        f.test
            .app
            .statements
            .update_line(&f.ctx, &draft.import.id, 0, update)
            .await
            .unwrap();
    }
    let group = created(&f, &commit(&f, &draft).await, 0).await;
    assert_eq!(group.splits[0].description, "Amazon: Moka pot + 1 more");
}

#[tokio::test]
async fn the_export_holds_profiles_sources_and_their_booked_matches() {
    let f = fixture("export").await;
    amazon_order(&f).await;
    let done = commit(
        &f,
        &statement(&f, &format!("20/11/2026;AMAZON {ORDER};-20,00\n")).await,
    )
    .await;
    let doc = f.test.app.export.json(&f.ctx).await.unwrap();
    assert_eq!(doc.statement_profiles.len(), 1);
    assert_eq!(doc.statement_profiles[0].name, "Bank");
    assert_eq!(doc.source_documents.len(), 1);
    assert_eq!(doc.source_rows.len(), 2);
    let split = done.lines[0].split_id.clone().unwrap();
    assert_eq!(doc.source_matches.len(), 2);
    assert!(
        doc.source_matches
            .iter()
            .all(|m| m.split_id == split && m.match_type == "order_id")
    );
}
