#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

//! Committing and reverting statement imports (Phase 13, Track B; brief
//! section 9 acceptance criteria).

mod common;

use std::collections::BTreeMap;

use common::*;
use saldo_app::accounts::{AccountFilter, AccountUpdate, AccountView};
use saldo_app::categories::NewCategory;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::page::PageRequest;
use saldo_app::reconciliations::NewReconciliation;
use saldo_app::rules::{ActionView, NewRuleGroup, RuleInput, TriggerView};
use saldo_app::statements::parse::{AmountColumns, Mapping};
use saldo_app::statements::{ImportDetail, LineUpdate, ProfileInput, ProfileView, UploadInput};
use saldo_app::transactions::{TransactionFilter, TransactionGroupView};

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn csv_mapping() -> Mapping {
    let mut mapping = Mapping::mediobanca();
    mapping.date_column = names(&["Data"]);
    mapping.value_date_column = None;
    mapping.amount = AmountColumns::Signed {
        column: names(&["Importo"]),
    };
    mapping.description_columns = vec![names(&["Descrizione"])];
    mapping.currency_column = None;
    mapping.counterparty_column = Some(names(&["Controparte"]));
    mapping.counterparty_iban_column = Some(names(&["IBAN"]));
    mapping
}

const HEADER: &str = "Data;Descrizione;Importo;Controparte;IBAN\n";

struct Fixture {
    test: TestApp,
    ctx: HouseholdCtx,
    checking: AccountView,
    savings: AccountView,
    checking_profile: ProfileView,
    savings_profile: ProfileView,
}

async fn profile(
    test: &TestApp,
    ctx: &HouseholdCtx,
    name: &str,
    account: &AccountView,
    mapping: Mapping,
) -> ProfileView {
    let input = ProfileInput {
        name: name.to_string(),
        account_id: Some(account.id.clone()),
        format: "csv".to_string(),
        mapping,
        part_accounts: BTreeMap::new(),
        transfer_rules: vec![],
    };
    test.app
        .statements
        .create_profile(ctx, Origin::Api, input)
        .await
        .unwrap()
}

async fn fixture() -> Fixture {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let savings = account(&test, &ctx, "Savings", "asset", None).await;
    for (account, iban) in [(&checking, "IT11CHECKING"), (&savings, "IT22SAVINGS")] {
        let update = AccountUpdate {
            iban: Some(Some(iban.to_string())),
            ..AccountUpdate::default()
        };
        test.app
            .accounts
            .update(&ctx, Origin::Api, account_id(account), update)
            .await
            .unwrap();
    }
    let checking_profile = profile(&test, &ctx, "Checking", &checking, csv_mapping()).await;
    let savings_profile = profile(&test, &ctx, "Savings", &savings, csv_mapping()).await;
    Fixture {
        test,
        ctx,
        checking,
        savings,
        checking_profile,
        savings_profile,
    }
}

async fn upload_with(f: &Fixture, profile: &ProfileView, body: &str) -> ImportDetail {
    let input = UploadInput {
        account_id: None,
        profile_id: profile.id.clone(),
        filename: "statement.csv".to_string(),
        bytes: format!("{HEADER}{body}").into_bytes(),
    };
    f.test
        .app
        .statements
        .upload(&f.ctx, Origin::Api, input)
        .await
        .unwrap()
}

async fn commit(f: &Fixture, detail: &ImportDetail) -> ImportDetail {
    f.test
        .app
        .statements
        .commit(&f.ctx, Origin::Api, &detail.import.id)
        .await
        .unwrap()
}

async fn groups_of(f: &Fixture, account: &AccountView) -> Vec<TransactionGroupView> {
    let filter = TransactionFilter {
        account_id: Some(account_id(account)),
        ..TransactionFilter::default()
    };
    f.test
        .app
        .transactions
        .list(&f.ctx, &filter, PageRequest::default())
        .await
        .unwrap()
        .items
}

fn statuses(detail: &ImportDetail) -> Vec<&str> {
    detail.lines.iter().map(|l| l.status.as_str()).collect()
}

#[tokio::test]
async fn committing_creates_links_and_skips_with_events() {
    let f = fixture().await;
    let shop = account(&f.test, &f.ctx, "Shop", "expense", None).await;
    let existing = f
        .test
        .app
        .transactions
        .create(
            &f.ctx,
            Origin::Api,
            group(vec![split(5, &f.checking, &shop, "20.00")]),
        )
        .await
        .unwrap();
    let draft = upload_with(&f, &f.checking_profile, "05/11/2026;POS Shop;-20,00;Shop;\n06/11/2026;Bar Sport;-3,50;Bar Sport;\n07/11/2026;Ignore me;-1,00;;\n").await;
    assert_eq!(statuses(&draft), vec!["possible_match", "new", "new"]);
    let skip = LineUpdate {
        action: Some("skip".to_string()),
        ..LineUpdate::default()
    };
    f.test
        .app
        .statements
        .update_line(&f.ctx, &draft.import.id, 2, skip)
        .await
        .unwrap();

    let done = commit(&f, &draft).await;
    assert_eq!(done.import.status, "committed");
    assert!(done.import.committed_at.is_some());
    assert_eq!(
        done.lines[0].split_id.as_deref(),
        Some(existing.splits[0].id.as_str())
    );
    assert!(done.lines[0].created_group_id.is_none());
    let created = done.lines[1].created_group_id.clone().unwrap();
    assert!(done.lines[2].created_group_id.is_none());

    let group = f
        .test
        .app
        .transactions
        .get(&f.ctx, created.parse().unwrap())
        .await
        .unwrap();
    let split = &group.splits[0];
    assert_eq!(
        (
            split.date.as_str(),
            split.amount.as_str(),
            split.description.as_str()
        ),
        ("2026-11-06", "3.50", "Bar Sport")
    );
    assert_eq!(split.source_account_id, f.checking.id);
    assert!(split.notes.as_deref().unwrap().contains("Bar Sport"));
    assert_eq!(split.meta["statement"]["import_id"], done.import.id);
    // The new counterparty is an expense account named after it.
    let bar = &f
        .test
        .app
        .accounts
        .get(&f.ctx, split.destination_account_id.parse().unwrap())
        .await
        .unwrap();
    assert_eq!(
        (bar.name.as_str(), bar.account_type.as_str()),
        ("Bar Sport", "expense")
    );

    let types: Vec<String> = events(&f.test, &f.ctx)
        .await
        .into_iter()
        .map(|e| e.event_type)
        .collect();
    assert!(
        types.ends_with(&[
            "statement_import.committed".to_string(),
            "import.completed".to_string()
        ]),
        "{types:?}"
    );
    let again = f
        .test
        .app
        .statements
        .commit(&f.ctx, Origin::Api, &draft.import.id)
        .await;
    assert!(matches!(again, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn importing_the_same_or_an_overlapping_file_again_creates_only_new_lines() {
    let f = fixture().await;
    let first = upload_with(
        &f,
        &f.checking_profile,
        "05/11/2026;Coffee;-1,50;;\n05/11/2026;Coffee;-1,50;;\n06/11/2026;Skip;-9,00;;\n",
    )
    .await;
    let skip = LineUpdate {
        action: Some("skip".to_string()),
        ..LineUpdate::default()
    };
    f.test
        .app
        .statements
        .update_line(&f.ctx, &first.import.id, 2, skip)
        .await
        .unwrap();
    commit(&f, &first).await;
    assert_eq!(groups_of(&f, &f.checking).await.len(), 2);

    let same = upload_with(
        &f,
        &f.checking_profile,
        "05/11/2026;Coffee;-1,50;;\n05/11/2026;Coffee;-1,50;;\n06/11/2026;Skip;-9,00;;\n",
    )
    .await;
    assert_eq!(
        statuses(&same),
        vec!["duplicate", "duplicate", "skipped_before"]
    );
    let overlap = upload_with(
        &f,
        &f.checking_profile,
        "05/11/2026;Coffee;-1,50;;\n05/11/2026;Coffee;-1,50;;\n07/11/2026;Lunch;-12,00;;\n",
    )
    .await;
    assert_eq!(statuses(&overlap), vec!["duplicate", "duplicate", "new"]);
    commit(&f, &overlap).await;
    assert_eq!(groups_of(&f, &f.checking).await.len(), 3);
}

#[tokio::test]
async fn a_transfer_imported_from_both_statements_is_one_transfer() {
    let f = fixture().await;
    let out = upload_with(
        &f,
        &f.checking_profile,
        "10/11/2026;Giroconto;-100,00;;IT22SAVINGS\n",
    )
    .await;
    assert_eq!(
        out.lines[0].counterparty_account_id.as_deref(),
        Some(f.savings.id.as_str())
    );
    commit(&f, &out).await;
    let incoming = upload_with(
        &f,
        &f.savings_profile,
        "11/11/2026;Giroconto da conto;100,00;;IT11CHECKING\n",
    )
    .await;
    assert_eq!(statuses(&incoming), vec!["possible_match"]);
    commit(&f, &incoming).await;
    let transfers = groups_of(&f, &f.savings).await;
    assert_eq!(transfers.len(), 1);
    assert_eq!(transfers[0].splits[0].kind, "transfer");
}

#[tokio::test]
async fn rules_run_on_imported_lines() {
    let f = fixture().await;
    let food = f
        .test
        .app
        .categories
        .create(
            &f.ctx,
            Origin::Api,
            NewCategory {
                name: "Food".into(),
                notes: None,
                color: None,
            },
        )
        .await
        .unwrap();
    let rule_group = f
        .test
        .app
        .rules
        .create_group(
            &f.ctx,
            Origin::Api,
            NewRuleGroup {
                title: "Default".into(),
                active: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let rule = RuleInput {
        rule_group_id: rule_group.id,
        title: "Bakery".into(),
        description: None,
        trigger_on: "create".into(),
        strict: true,
        stop_processing: false,
        active: true,
        sort_order: 0,
        triggers: vec![TriggerView {
            kind: "description_contains".into(),
            value: Some("bakery".into()),
            negate: false,
        }],
        actions: vec![ActionView {
            kind: "set_category".into(),
            value: Some(food.id.clone()),
            stop_processing: false,
            active: true,
        }],
    };
    f.test
        .app
        .rules
        .create_rule(&f.ctx, Origin::Api, rule)
        .await
        .unwrap();
    let done = commit(
        &f,
        &upload_with(
            &f,
            &f.checking_profile,
            "05/11/2026;POS BAKERY ROSSI;-4,00;;\n",
        )
        .await,
    )
    .await;
    let group = f
        .test
        .app
        .transactions
        .get(
            &f.ctx,
            done.lines[0]
                .created_group_id
                .clone()
                .unwrap()
                .parse()
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        group.splits[0].category_id.as_deref(),
        Some(food.id.as_str())
    );
}

#[tokio::test]
async fn a_satispay_payment_with_meal_vouchers_is_one_split_transaction() {
    let f = fixture().await;
    let satispay = account(&f.test, &f.ctx, "Satispay", "asset", None).await;
    let vouchers = account(&f.test, &f.ctx, "Meal vouchers", "asset", None).await;
    let mut parts = BTreeMap::new();
    parts.insert("Meal Vouchers".to_string(), Some(vouchers.id.clone()));
    parts.insert("Shopping Vouchers".to_string(), None);
    let input = ProfileInput {
        name: "Satispay".to_string(),
        account_id: Some(satispay.id.clone()),
        format: "csv".to_string(),
        mapping: Mapping::satispay(),
        part_accounts: parts,
        transfer_rules: vec![],
    };
    let profile = f
        .test
        .app
        .statements
        .create_profile(&f.ctx, Origin::Api, input)
        .await
        .unwrap();
    let csv = "Date;Name;Balance;Meal Vouchers;Shopping Vouchers;Balance after transaction;Type;Status;ID\n\
               05/11/2026 09:00;Bar;-5.00;-8.00;-1.00;10.00;x;ok;S1\n\
               06/11/2026 09:00;Canteen;;-8.00;;10.00;x;ok;S2\n";
    let upload = UploadInput {
        account_id: None,
        profile_id: profile.id,
        filename: "satispay.csv".to_string(),
        bytes: csv.as_bytes().to_vec(),
    };
    let draft = f
        .test
        .app
        .statements
        .upload(&f.ctx, Origin::Api, upload)
        .await
        .unwrap();
    let done = commit(&f, &draft).await;

    let mixed = f
        .test
        .app
        .transactions
        .get(
            &f.ctx,
            done.lines[0]
                .created_group_id
                .clone()
                .unwrap()
                .parse()
                .unwrap(),
        )
        .await
        .unwrap();
    let summary: Vec<(&str, &str)> = mixed
        .splits
        .iter()
        .map(|s| (s.source_account_id.as_str(), s.amount.as_str()))
        .collect();
    assert_eq!(
        summary,
        vec![
            (satispay.id.as_str(), "5.00"),
            (vouchers.id.as_str(), "8.00")
        ]
    );
    assert_eq!(
        mixed.splits[0].destination_account_id,
        mixed.splits[1].destination_account_id
    );
    assert_eq!(mixed.title.as_deref(), Some("Bar"));
    // Shopping vouchers are mapped to nothing: a note on the payment.
    assert!(
        mixed.splits[0]
            .notes
            .as_deref()
            .unwrap()
            .contains("Shopping Vouchers: -1.00")
    );

    let vouchers_only = f
        .test
        .app
        .transactions
        .get(
            &f.ctx,
            done.lines[1]
                .created_group_id
                .clone()
                .unwrap()
                .parse()
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(vouchers_only.splits.len(), 1);
    assert_eq!(vouchers_only.splits[0].source_account_id, vouchers.id);
}

#[tokio::test]
async fn reverting_deletes_exactly_what_the_import_created() {
    let f = fixture().await;
    let shop = account(&f.test, &f.ctx, "Shop", "expense", None).await;
    let mine = f
        .test
        .app
        .transactions
        .create(
            &f.ctx,
            Origin::Api,
            group(vec![split(5, &f.checking, &shop, "20.00")]),
        )
        .await
        .unwrap();
    let draft = upload_with(
        &f,
        &f.checking_profile,
        "05/11/2026;Shop;-20,00;;\n06/11/2026;New;-3,00;;\n",
    )
    .await;
    let done = commit(&f, &draft).await;
    assert_eq!(groups_of(&f, &f.checking).await.len(), 2);

    let reverted = f
        .test
        .app
        .statements
        .revert(&f.ctx, Origin::Api, &done.import.id)
        .await
        .unwrap();
    assert_eq!(reverted.status, "reverted");
    assert_eq!(
        last_event(&f.test, &f.ctx).await.event_type,
        "statement_import.reverted"
    );
    let left = groups_of(&f, &f.checking).await;
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, mine.id);
    // Both links are gone, so the same file is new again.
    let again = upload_with(
        &f,
        &f.checking_profile,
        "05/11/2026;Shop;-20,00;;\n06/11/2026;New;-3,00;;\n",
    )
    .await;
    assert_eq!(statuses(&again), vec!["possible_match", "new"]);
    let twice = f
        .test
        .app
        .statements
        .revert(&f.ctx, Origin::Api, &done.import.id)
        .await;
    assert!(matches!(twice, Err(AppError::Conflict(_))));
    let draft_revert = f
        .test
        .app
        .statements
        .revert(&f.ctx, Origin::Api, &again.import.id)
        .await;
    assert!(matches!(draft_revert, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn reverting_is_refused_when_a_created_transaction_is_reconciled() {
    let f = fixture().await;
    let done = commit(
        &f,
        &upload_with(&f, &f.checking_profile, "05/11/2026;New;-3,00;;\n").await,
    )
    .await;
    let created = f
        .test
        .app
        .transactions
        .get(
            &f.ctx,
            done.lines[0]
                .created_group_id
                .clone()
                .unwrap()
                .parse()
                .unwrap(),
        )
        .await
        .unwrap();
    let reconcile = NewReconciliation {
        statement_date: date(30),
        statement_balance: "-3.00".to_string(),
        split_ids: vec![created.splits[0].id.clone()],
        book_difference: false,
    };
    f.test
        .app
        .reconciliations
        .create(&f.ctx, Origin::Api, account_id(&f.checking), reconcile)
        .await
        .unwrap();
    let result = f
        .test
        .app
        .statements
        .revert(&f.ctx, Origin::Api, &done.import.id)
        .await;
    assert!(matches!(result, Err(AppError::Conflict(_))), "{result:?}");
    assert_eq!(groups_of(&f, &f.checking).await.len(), 1);
}

#[tokio::test]
async fn a_new_counterparty_named_twice_becomes_one_account() {
    let f = fixture().await;
    commit(&f, &upload_with(&f, &f.checking_profile, "05/11/2026;POS 1;-3,00;Bar Sport;\n06/11/2026;POS 2;-4,00;Bar Sport;\n07/11/2026;Stipendio;1000,00;ACME Srl;\n").await).await;
    let filter = AccountFilter {
        account_type: Some("expense".to_string()),
        ..AccountFilter::default()
    };
    let expense: Vec<String> = f
        .test
        .app
        .accounts
        .list(&f.ctx, &filter, PageRequest::default())
        .await
        .unwrap()
        .items
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert_eq!(expense, vec!["Bar Sport"]);
    let filter = AccountFilter {
        account_type: Some("revenue".to_string()),
        ..AccountFilter::default()
    };
    let revenue: Vec<String> = f
        .test
        .app
        .accounts
        .list(&f.ctx, &filter, PageRequest::default())
        .await
        .unwrap()
        .items
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert_eq!(revenue, vec!["ACME Srl"]);
}

#[tokio::test]
async fn only_skips_the_owner_chose_are_remembered() {
    let f = fixture().await;
    let first = commit(
        &f,
        &upload_with(&f, &f.checking_profile, "05/11/2026;Coffee;-1,50;;\n").await,
    )
    .await;
    // The same file again: the line is a duplicate, skipped automatically.
    commit(
        &f,
        &upload_with(&f, &f.checking_profile, "05/11/2026;Coffee;-1,50;;\n").await,
    )
    .await;
    // The owner deletes the transaction: the line must come back (decision 11).
    let group = first.lines[0].created_group_id.clone().unwrap();
    f.test
        .app
        .transactions
        .delete(&f.ctx, Origin::Api, group.parse().unwrap())
        .await
        .unwrap();
    let third = upload_with(&f, &f.checking_profile, "05/11/2026;Coffee;-1,50;;\n").await;
    assert_eq!(statuses(&third), vec!["new"]);
}

#[tokio::test]
async fn two_drafts_of_one_file_cannot_both_be_committed() {
    let f = fixture().await;
    let a = upload_with(&f, &f.checking_profile, "05/11/2026;Coffee;-1,50;;\n").await;
    let b = upload_with(&f, &f.checking_profile, "05/11/2026;Coffee;-1,50;;\n").await;
    commit(&f, &a).await;
    let second = f
        .test
        .app
        .statements
        .commit(&f.ctx, Origin::Api, &b.import.id)
        .await;
    assert!(matches!(second, Err(AppError::Conflict(_))), "{second:?}");
    assert_eq!(groups_of(&f, &f.checking).await.len(), 1);
}

#[tokio::test]
async fn a_duplicate_forced_to_link_elsewhere_is_refused_and_the_old_link_survives() {
    let f = fixture().await;
    let shop = account(&f.test, &f.ctx, "Shop", "expense", None).await;
    let other = f
        .test
        .app
        .transactions
        .create(
            &f.ctx,
            Origin::Api,
            group(vec![split(20, &f.checking, &shop, "9.00")]),
        )
        .await
        .unwrap();
    commit(
        &f,
        &upload_with(&f, &f.checking_profile, "05/11/2026;Coffee;-1,50;;\n").await,
    )
    .await;
    let again = upload_with(&f, &f.checking_profile, "05/11/2026;Coffee;-1,50;;\n").await;
    let force = LineUpdate {
        action: Some("link".to_string()),
        split_id: Some(Some(other.splits[0].id.clone())),
        ..LineUpdate::default()
    };
    f.test
        .app
        .statements
        .update_line(&f.ctx, &again.import.id, 0, force)
        .await
        .unwrap();
    let result = f
        .test
        .app
        .statements
        .commit(&f.ctx, Origin::Api, &again.import.id)
        .await;
    assert!(matches!(result, Err(AppError::Conflict(_))), "{result:?}");
    let later = upload_with(&f, &f.checking_profile, "05/11/2026;Coffee;-1,50;;\n").await;
    assert_eq!(statuses(&later), vec!["duplicate"]);
}

#[tokio::test]
async fn a_voucher_only_payment_gets_an_expense_counterparty_and_books() {
    let f = fixture().await;
    let satispay = account(&f.test, &f.ctx, "Satispay", "asset", None).await;
    let vouchers = account(&f.test, &f.ctx, "Meal vouchers", "asset", None).await;
    account(&f.test, &f.ctx, "Canteen", "revenue", None).await;
    let canteen = account(&f.test, &f.ctx, "Canteen", "expense", None).await;
    let mut parts = BTreeMap::new();
    parts.insert("Meal Vouchers".to_string(), Some(vouchers.id.clone()));
    let input = ProfileInput {
        name: "Satispay".to_string(),
        account_id: Some(satispay.id.clone()),
        format: "csv".to_string(),
        mapping: Mapping::satispay(),
        part_accounts: parts,
        transfer_rules: vec![],
    };
    let profile = f
        .test
        .app
        .statements
        .create_profile(&f.ctx, Origin::Api, input)
        .await
        .unwrap();
    let csv = "Date;Name;Balance;Meal Vouchers;Shopping Vouchers;Balance after transaction;Type;Status;ID\n\
               06/11/2026 09:00;Canteen;;-8.00;;10.00;x;ok;S2\n";
    let upload = UploadInput {
        account_id: None,
        profile_id: profile.id.clone(),
        filename: "s.csv".to_string(),
        bytes: csv.as_bytes().to_vec(),
    };
    let draft = f
        .test
        .app
        .statements
        .upload(&f.ctx, Origin::Api, upload)
        .await
        .unwrap();
    assert_eq!(
        draft.lines[0].counterparty_account_id.as_deref(),
        Some(canteen.id.as_str())
    );
    // The profile is deleted before the commit: the draft keeps the mapping
    // it was reviewed with.
    f.test
        .app
        .statements
        .delete_profile(&f.ctx, Origin::Api, &profile.id)
        .await
        .unwrap();
    let done = commit(&f, &draft).await;
    let group = f
        .test
        .app
        .transactions
        .get(
            &f.ctx,
            done.lines[0]
                .created_group_id
                .clone()
                .unwrap()
                .parse()
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(group.splits[0].source_account_id, vouchers.id);
    assert_eq!(group.splits[0].destination_account_id, canteen.id);
}

#[tokio::test]
async fn a_line_mixing_money_in_and_out_is_refused_by_row() {
    let f = fixture().await;
    let satispay = account(&f.test, &f.ctx, "Satispay", "asset", None).await;
    let vouchers = account(&f.test, &f.ctx, "Meal vouchers", "asset", None).await;
    let mut parts = BTreeMap::new();
    parts.insert("Meal Vouchers".to_string(), Some(vouchers.id.clone()));
    let input = ProfileInput {
        name: "Satispay".to_string(),
        account_id: Some(satispay.id.clone()),
        format: "csv".to_string(),
        mapping: Mapping::satispay(),
        part_accounts: parts,
        transfer_rules: vec![],
    };
    let profile = f
        .test
        .app
        .statements
        .create_profile(&f.ctx, Origin::Api, input)
        .await
        .unwrap();
    let csv = "Date;Name;Balance;Meal Vouchers;Shopping Vouchers;Balance after transaction;Type;Status;ID\n\
               06/11/2026 09:00;Odd;-10.00;3.00;;10.00;x;ok;S9\n";
    let upload = UploadInput {
        account_id: None,
        profile_id: profile.id,
        filename: "s.csv".to_string(),
        bytes: csv.as_bytes().to_vec(),
    };
    let draft = f
        .test
        .app
        .statements
        .upload(&f.ctx, Origin::Api, upload)
        .await
        .unwrap();
    let result = f
        .test
        .app
        .statements
        .commit(&f.ctx, Origin::Api, &draft.import.id)
        .await;
    assert!(
        matches!(&result, Err(AppError::Invalid { field, message }) if field == "lines" && message.contains("row 2")),
        "{result:?}"
    );
}
