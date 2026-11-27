#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

//! Statement imports as drafts: upload, classification, review edits and
//! discarding (Phase 13, Track B).

mod common;

use std::collections::BTreeMap;

use common::*;
use saldo_app::accounts::{AccountUpdate, AccountView};
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::external_refs::{BANK_STATEMENT, ExternalRefs};
use saldo_app::reconciliations::NewReconciliation;
use saldo_app::statements::parse::{AmountColumns, Mapping};
use saldo_app::statements::{
    ImportDetail, LineUpdate, ProfileInput, ProfileView, TransferRule, UploadInput,
};

/// A plain CSV bank: date, text, signed amount, optional ID and IBAN.
fn csv_mapping() -> Mapping {
    let names = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let mut mapping = Mapping::mediobanca();
    mapping.date_column = names(&["Data"]);
    mapping.value_date_column = None;
    mapping.amount = AmountColumns::Signed {
        column: names(&["Importo"]),
    };
    mapping.description_columns = vec![names(&["Descrizione"])];
    mapping.currency_column = None;
    mapping.external_id_column = Some(names(&["ID"]));
    mapping.counterparty_column = Some(names(&["Controparte"]));
    mapping.counterparty_iban_column = Some(names(&["IBAN"]));
    mapping
}

struct Fixture {
    test: TestApp,
    ctx: HouseholdCtx,
    checking: AccountView,
    savings: AccountView,
    satispay: AccountView,
    shop: AccountView,
    profile: ProfileView,
}

async fn fixture() -> Fixture {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let savings = account(&test, &ctx, "Savings", "asset", None).await;
    let satispay = account(&test, &ctx, "Satispay", "asset", None).await;
    let shop = account(&test, &ctx, "Bakery Rossi", "expense", None).await;
    let iban = AccountUpdate {
        iban: Some(Some("IT60X0542811101000000123456".to_string())),
        ..AccountUpdate::default()
    };
    test.app
        .accounts
        .update(&ctx, Origin::Api, account_id(&savings), iban)
        .await
        .unwrap();
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
                mapping: csv_mapping(),
                part_accounts: BTreeMap::new(),
                transfer_rules: vec![TransferRule {
                    pattern: "satispay".to_string(),
                    account_ids: vec![satispay.id.clone()],
                }],
            },
        )
        .await
        .unwrap();
    Fixture {
        test,
        ctx,
        checking,
        savings,
        satispay,
        shop,
        profile,
    }
}

const HEADER: &str = "Data;Descrizione;Importo;ID;Controparte;IBAN\n";

async fn upload(f: &Fixture, body: &str) -> Result<ImportDetail, AppError> {
    let input = UploadInput {
        account_id: None,
        profile_id: f.profile.id.clone(),
        filename: "november.csv".to_string(),
        bytes: format!("{HEADER}{body}").into_bytes(),
    };
    f.test
        .app
        .statements
        .upload(&f.ctx, Origin::Api, input)
        .await
}

async fn spend(f: &Fixture, day: u32, amount: &str) -> String {
    let view = f
        .test
        .app
        .transactions
        .create(
            &f.ctx,
            Origin::Api,
            group(vec![split(day, &f.checking, &f.shop, amount)]),
        )
        .await
        .unwrap();
    view.splits[0].id.clone()
}

fn statuses(detail: &ImportDetail) -> Vec<(String, String)> {
    detail
        .lines
        .iter()
        .map(|l| (l.status.clone(), l.action.clone()))
        .collect()
}

#[tokio::test]
async fn a_fresh_file_is_a_draft_of_new_lines_with_unsupported_rows_in_the_summary() {
    let f = fixture().await;
    let detail = upload(&f, "10/11/2026;Coffee;-1,50;A1;;\nTotale;;-1,50;;;\n")
        .await
        .unwrap();
    assert_eq!(detail.import.status, "draft");
    assert_eq!(detail.import.account_id, f.checking.id);
    assert_eq!(
        statuses(&detail),
        vec![("new".to_string(), "create".to_string())]
    );
    let line = &detail.lines[0];
    assert_eq!(line.amount, "-1.50");
    assert_eq!(line.booked_date, "2026-11-10");
    assert_eq!(line.ref_key, format!("{}:A1", f.checking.id));
    assert_eq!(detail.import.summary["unsupported"][0]["row"], 3);
    assert_eq!(detail.import.summary["counts"]["new"], 1);
    assert_eq!(
        last_event(&f.test, &f.ctx).await.event_type,
        "statement_import.created"
    );
}

#[tokio::test]
async fn an_existing_transaction_is_a_possible_match_one_to_one() {
    let f = fixture().await;
    let first = spend(&f, 9, "12.99").await;
    let second = spend(&f, 30, "12.99").await;
    let detail = upload(
        &f,
        "10/11/2026;Netflix;-12,99;;;\n29/11/2026;Netflix;-12,99;;;\n",
    )
    .await
    .unwrap();
    assert_eq!(
        statuses(&detail),
        vec![("possible_match".to_string(), "link".to_string()); 2]
    );
    assert_eq!(detail.lines[0].split_id.as_deref(), Some(first.as_str()));
    assert_eq!(detail.lines[1].split_id.as_deref(), Some(second.as_str()));
    assert!(detail.lines[0].reason.as_deref().unwrap().contains("1 day"));
}

#[tokio::test]
async fn a_split_already_linked_to_a_bank_line_is_no_candidate_but_a_firefly_one_is() {
    let f = fixture().await;
    let linked = spend(&f, 10, "5.00").await;
    let from_firefly = spend(&f, 10, "7.00").await;
    let mut conn = f.test.app.pool.acquire().await.unwrap();
    let mut statement = ExternalRefs::load(&mut conn, &f.ctx, BANK_STATEMENT)
        .await
        .unwrap();
    statement
        .upsert(
            &mut conn,
            "split",
            &format!("{}:other-line", f.checking.id),
            &linked,
            "t",
        )
        .await
        .unwrap();
    let mut firefly = ExternalRefs::load(&mut conn, &f.ctx, saldo_app::external_refs::FIREFLY)
        .await
        .unwrap();
    firefly
        .upsert(&mut conn, "split", "99", &from_firefly, "t")
        .await
        .unwrap();
    drop(conn);
    let detail = upload(&f, "10/11/2026;A;-5,00;;;\n10/11/2026;B;-7,00;;;\n")
        .await
        .unwrap();
    assert_eq!(detail.lines[0].status, "new");
    assert_eq!(detail.lines[1].status, "possible_match");
    assert_eq!(
        detail.lines[1].split_id.as_deref(),
        Some(from_firefly.as_str())
    );
}

#[tokio::test]
async fn a_line_already_imported_is_a_duplicate() {
    let f = fixture().await;
    let existing = spend(&f, 10, "1.50").await;
    let mut conn = f.test.app.pool.acquire().await.unwrap();
    let mut refs = ExternalRefs::load(&mut conn, &f.ctx, BANK_STATEMENT)
        .await
        .unwrap();
    refs.upsert(
        &mut conn,
        "split",
        &format!("{}:A1", f.checking.id),
        &existing,
        "t",
    )
    .await
    .unwrap();
    drop(conn);
    let detail = upload(&f, "10/11/2026;Coffee;-1,50;A1;;\n").await.unwrap();
    assert_eq!(
        statuses(&detail),
        vec![("duplicate".to_string(), "skip".to_string())]
    );
    assert_eq!(detail.lines[0].split_id.as_deref(), Some(existing.as_str()));
}

#[tokio::test]
async fn an_unmatched_line_in_a_reconciled_period_is_flagged_not_created() {
    let f = fixture().await;
    let input = NewReconciliation {
        statement_date: date(15),
        statement_balance: "0.00".to_string(),
        split_ids: vec![],
        book_difference: false,
    };
    f.test
        .app
        .reconciliations
        .create(&f.ctx, Origin::Api, account_id(&f.checking), input)
        .await
        .unwrap();
    let detail = upload(&f, "10/11/2026;Old;-3,00;;;\n20/11/2026;New;-4,00;;;\n")
        .await
        .unwrap();
    assert_eq!(
        statuses(&detail),
        vec![
            ("not_matched_reconciled".to_string(), "skip".to_string()),
            ("new".to_string(), "create".to_string()),
        ]
    );
}

#[tokio::test]
async fn leftover_transactions_in_the_statement_period_are_findings() {
    let f = fixture().await;
    let matched = spend(&f, 10, "9.00").await;
    let copy = spend(&f, 11, "9.00").await;
    let missing = spend(&f, 12, "33.00").await;
    let detail = upload(
        &f,
        "10/11/2026;Bakery;-9,00;;;\n14/11/2026;Other;-1,00;;;\n",
    )
    .await
    .unwrap();
    let findings = &detail.import.summary["findings"];
    assert_eq!(findings[0]["kind"], "duplicate_of");
    assert_eq!(findings[0]["split_ids"][0], copy);
    assert_eq!(findings[0]["of_split_ids"][0], matched);
    assert_eq!(findings[1]["kind"], "not_on_statement");
    assert_eq!(findings[1]["split_ids"][0], missing);
}

#[tokio::test]
async fn counterparties_are_proposed_from_ibans_rules_and_names() {
    let f = fixture().await;
    let body = "\
10/11/2026;Giroconto;-100,00;;;IT60X0542811101000000123456\n\
11/11/2026;Ricarica SATISPAY;-20,00;;;\n\
12/11/2026;POS 1234;-4,50;;Bakery Rossi;\n\
13/11/2026;POS 5678;-6,00;;Unknown shop;\n";
    let detail = upload(&f, body).await.unwrap();
    let proposed: Vec<Option<String>> = detail
        .lines
        .iter()
        .map(|l| l.counterparty_account_id.clone())
        .collect();
    assert_eq!(
        proposed,
        vec![
            Some(f.savings.id.clone()),
            Some(f.satispay.id.clone()),
            Some(f.shop.id.clone()),
            None
        ]
    );
}

#[tokio::test]
async fn review_edits_change_a_draft_line_and_are_checked() {
    let f = fixture().await;
    let existing = spend(&f, 20, "8.00").await;
    let detail = upload(&f, "10/11/2026;Coffee;-1,50;;;\n").await.unwrap();
    let id = detail.import.id.clone();
    let statements = &f.test.app.statements;

    let skip = LineUpdate {
        action: Some("skip".to_string()),
        ..LineUpdate::default()
    };
    let line = statements.update_line(&f.ctx, &id, 0, skip).await.unwrap();
    assert_eq!(line.action, "skip");

    let link = LineUpdate {
        action: Some("link".to_string()),
        split_id: Some(Some(existing.clone())),
        ..LineUpdate::default()
    };
    let line = statements.update_line(&f.ctx, &id, 0, link).await.unwrap();
    assert_eq!(
        (line.action.as_str(), line.split_id.as_deref()),
        ("link", Some(existing.as_str()))
    );

    let shop = LineUpdate {
        counterparty_account_id: Some(Some(f.shop.id.clone())),
        ..LineUpdate::default()
    };
    assert_eq!(
        statements
            .update_line(&f.ctx, &id, 0, shop)
            .await
            .unwrap()
            .counterparty_account_id,
        Some(f.shop.id.clone())
    );

    let link_without_split = LineUpdate {
        action: Some("link".to_string()),
        split_id: Some(None),
        ..LineUpdate::default()
    };
    assert!(matches!(
        statements
            .update_line(&f.ctx, &id, 0, link_without_split)
            .await,
        Err(AppError::Invalid { .. })
    ));
    let foreign_split = LineUpdate {
        split_id: Some(Some("nope".to_string())),
        ..LineUpdate::default()
    };
    assert!(matches!(
        statements.update_line(&f.ctx, &id, 0, foreign_split).await,
        Err(AppError::Invalid { .. })
    ));
    let bad_action = LineUpdate {
        action: Some("delete".to_string()),
        ..LineUpdate::default()
    };
    assert!(matches!(
        statements.update_line(&f.ctx, &id, 0, bad_action).await,
        Err(AppError::Invalid { .. })
    ));
    assert!(matches!(
        statements
            .update_line(&f.ctx, &id, 9, LineUpdate::default())
            .await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn a_discarded_draft_is_final_and_another_household_cannot_touch_it() {
    let f = fixture().await;
    let detail = upload(&f, "10/11/2026;Coffee;-1,50;;;\n").await.unwrap();
    let id = detail.import.id.clone();
    let statements = &f.test.app.statements;
    let other = other_household(&f.test).await;
    assert!(matches!(
        statements.get(&other, &id).await,
        Err(AppError::NotFound)
    ));
    assert!(matches!(
        statements.discard(&other, Origin::Api, &id).await,
        Err(AppError::NotFound)
    ));

    let discarded = statements.discard(&f.ctx, Origin::Api, &id).await.unwrap();
    assert_eq!(discarded.status, "discarded");
    assert_eq!(
        last_event(&f.test, &f.ctx).await.event_type,
        "statement_import.discarded"
    );
    assert!(matches!(
        statements.discard(&f.ctx, Origin::Api, &id).await,
        Err(AppError::Conflict(_))
    ));
    let edit = statements
        .update_line(&f.ctx, &id, 0, LineUpdate::default())
        .await;
    assert!(matches!(edit, Err(AppError::Conflict(_))));

    let listed = statements
        .list(
            &f.ctx,
            Some(f.checking.id.clone()),
            saldo_app::page::PageRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(listed.items.len(), 1);
    assert_eq!(statements.get(&f.ctx, &id).await.unwrap().lines.len(), 1);
}

#[tokio::test]
async fn unreadable_files_and_wrong_accounts_are_refused() {
    let f = fixture().await;
    let statements = &f.test.app.statements;
    let bad = UploadInput {
        account_id: None,
        profile_id: f.profile.id.clone(),
        filename: "x.csv".to_string(),
        bytes: b"no header here\n".to_vec(),
    };
    let error = statements
        .upload(&f.ctx, Origin::Api, bad)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::Invalid { field, .. } if field == "file"),
        "{error:?}"
    );

    let expense = UploadInput {
        account_id: Some(f.shop.id.clone()),
        profile_id: f.profile.id.clone(),
        filename: "x.csv".to_string(),
        bytes: format!("{HEADER}10/11/2026;x;-1,00;;;\n").into_bytes(),
    };
    let error = statements
        .upload(&f.ctx, Origin::Api, expense)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::Invalid { field, .. } if field == "account_id"),
        "{error:?}"
    );
}

#[tokio::test]
async fn the_owners_widiba_model_uploads() {
    let f = fixture().await;
    let profile = f
        .test
        .app
        .statements
        .create_profile(
            &f.ctx,
            Origin::Api,
            ProfileInput {
                name: "Widiba".to_string(),
                account_id: Some(f.checking.id.clone()),
                format: "xlsx".to_string(),
                mapping: Mapping::widiba(),
                part_accounts: BTreeMap::new(),
                transfer_rules: vec![],
            },
        )
        .await
        .unwrap();
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/statements/widiba.xlsx"
    ))
    .unwrap();
    let input = UploadInput {
        account_id: None,
        profile_id: profile.id,
        filename: "widiba.xlsx".to_string(),
        bytes,
    };
    let detail = f
        .test
        .app
        .statements
        .upload(&f.ctx, Origin::Api, input)
        .await
        .unwrap();
    assert_eq!(detail.lines.len(), 1);
    assert_eq!(detail.lines[0].amount, "-41.76");
    assert_eq!(
        detail.import.summary["unsupported"][0]["reason"],
        "not a transaction"
    );
    assert_eq!(detail.import.gap, None);
}

#[tokio::test]
async fn the_filename_is_kept_as_a_plain_basename() {
    let f = fixture().await;
    let input = UploadInput {
        account_id: None,
        profile_id: f.profile.id.clone(),
        filename: "../../etc/\u{7}novem\u{0}ber.csv".to_string(),
        bytes: format!("{HEADER}10/11/2026;x;-1,00;;;\n").into_bytes(),
    };
    let detail = f
        .test
        .app
        .statements
        .upload(&f.ctx, Origin::Api, input)
        .await
        .unwrap();
    assert_eq!(detail.import.filename, "november.csv");
}
