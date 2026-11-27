#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

//! Statement profiles: how to read one bank's export (Phase 13, Track B).

mod common;

use std::collections::BTreeMap;

use common::*;
use saldo_app::accounts::AccountView;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::statements::parse::Mapping;
use saldo_app::statements::{ProfileInput, TransferRule, presets};

fn input(name: &str, account: Option<&AccountView>, mapping: Mapping) -> ProfileInput {
    ProfileInput {
        name: name.to_string(),
        account_id: account.map(|a| a.id.clone()),
        format: "xlsx".to_string(),
        mapping,
        part_accounts: BTreeMap::new(),
        transfer_rules: Vec::new(),
    }
}

struct Fixture {
    test: TestApp,
    ctx: HouseholdCtx,
    widiba: AccountView,
    satispay: AccountView,
    vouchers: AccountView,
    shop: AccountView,
}

async fn fixture() -> Fixture {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let widiba = account(&test, &ctx, "Widiba", "asset", None).await;
    let satispay = account(&test, &ctx, "Satispay", "asset", None).await;
    let vouchers = account(&test, &ctx, "Meal vouchers", "asset", None).await;
    let shop = account(&test, &ctx, "Shop", "expense", None).await;
    Fixture {
        test,
        ctx,
        widiba,
        satispay,
        vouchers,
        shop,
    }
}

#[test]
fn presets_cover_the_owners_banks() {
    let names: Vec<String> = presets().into_iter().map(|p| p.name).collect();
    assert_eq!(names, vec!["Widiba", "Mediobanca", "Satispay"]);
}

#[tokio::test]
async fn a_profile_is_created_listed_updated_and_deleted_with_events() {
    let f = fixture().await;
    let statements = &f.test.app.statements;
    let created = statements
        .create_profile(
            &f.ctx,
            Origin::Api,
            input("Widiba", Some(&f.widiba), Mapping::widiba()),
        )
        .await
        .unwrap();
    assert_eq!(created.name, "Widiba");
    assert_eq!(created.account_id.as_deref(), Some(f.widiba.id.as_str()));
    assert_eq!(created.format, "xlsx");
    assert_eq!(
        last_event(&f.test, &f.ctx).await.event_type,
        "statement_profile.created"
    );

    let mut satispay = input("Satispay", Some(&f.satispay), Mapping::satispay());
    satispay
        .part_accounts
        .insert("Meal Vouchers".to_string(), Some(f.vouchers.id.clone()));
    satispay
        .part_accounts
        .insert("Shopping Vouchers".to_string(), None);
    satispay.transfer_rules = vec![TransferRule {
        pattern: "From Bank".to_string(),
        account_ids: vec![f.widiba.id.clone()],
    }];
    statements
        .create_profile(&f.ctx, Origin::Api, satispay)
        .await
        .unwrap();
    let names: Vec<String> = statements
        .list_profiles(&f.ctx)
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(names, vec!["Satispay", "Widiba"]);

    let id = created.id.clone();
    let updated = statements
        .update_profile(
            &f.ctx,
            Origin::Api,
            &id,
            input("Widiba conto", None, Mapping::widiba()),
        )
        .await
        .unwrap();
    assert_eq!(updated.name, "Widiba conto");
    assert_eq!(updated.account_id, None);
    assert_eq!(
        last_event(&f.test, &f.ctx).await.event_type,
        "statement_profile.updated"
    );
    assert_eq!(statements.get_profile(&f.ctx, &id).await.unwrap(), updated);

    statements
        .delete_profile(&f.ctx, Origin::Api, &id)
        .await
        .unwrap();
    assert_eq!(
        last_event(&f.test, &f.ctx).await.event_type,
        "statement_profile.deleted"
    );
    assert!(matches!(
        statements.get_profile(&f.ctx, &id).await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn a_duplicate_name_is_a_conflict() {
    let f = fixture().await;
    let statements = &f.test.app.statements;
    statements
        .create_profile(
            &f.ctx,
            Origin::Api,
            input("Widiba", None, Mapping::widiba()),
        )
        .await
        .unwrap();
    let again = statements
        .create_profile(
            &f.ctx,
            Origin::Api,
            input(" Widiba ", None, Mapping::widiba()),
        )
        .await;
    assert!(matches!(again, Err(AppError::Conflict(_))), "{again:?}");
}

#[tokio::test]
async fn bad_profiles_are_refused_with_the_field_at_fault() {
    let f = fixture().await;
    let statements = &f.test.app.statements;
    let field = |result: Result<_, AppError>| match result {
        Err(AppError::Invalid { field, .. }) => field,
        other => panic!("expected Invalid, got {other:?}"),
    };

    let blank = input("  ", None, Mapping::widiba());
    assert_eq!(
        field(statements.create_profile(&f.ctx, Origin::Api, blank).await),
        "name"
    );

    let mut format = input("x", None, Mapping::widiba());
    format.format = "ofx".to_string();
    assert_eq!(
        field(statements.create_profile(&f.ctx, Origin::Api, format).await),
        "format"
    );

    let mut mapping = Mapping::widiba();
    mapping.pending_when = vec![String::new()];
    let bad_mapping = input("x", None, mapping);
    assert_eq!(
        field(
            statements
                .create_profile(&f.ctx, Origin::Api, bad_mapping)
                .await
        ),
        "mapping"
    );

    // A statement belongs to an asset or liability account.
    let expense = input("x", Some(&f.shop), Mapping::widiba());
    assert_eq!(
        field(
            statements
                .create_profile(&f.ctx, Origin::Api, expense)
                .await
        ),
        "account_id"
    );

    // A part column the mapping does not have, and a part account that is
    // not an asset account.
    let mut unknown_part = input("x", None, Mapping::satispay());
    unknown_part
        .part_accounts
        .insert("Tips".to_string(), Some(f.vouchers.id.clone()));
    assert_eq!(
        field(
            statements
                .create_profile(&f.ctx, Origin::Api, unknown_part)
                .await
        ),
        "part_accounts"
    );
    let mut expense_part = input("x", None, Mapping::satispay());
    expense_part
        .part_accounts
        .insert("Meal Vouchers".to_string(), Some(f.shop.id.clone()));
    assert_eq!(
        field(
            statements
                .create_profile(&f.ctx, Origin::Api, expense_part)
                .await
        ),
        "part_accounts"
    );

    let mut empty_rule = input("x", None, Mapping::widiba());
    empty_rule.transfer_rules = vec![TransferRule {
        pattern: " ".to_string(),
        account_ids: vec![f.satispay.id.clone()],
    }];
    assert_eq!(
        field(
            statements
                .create_profile(&f.ctx, Origin::Api, empty_rule)
                .await
        ),
        "transfer_rules"
    );
    let mut no_accounts = input("x", None, Mapping::widiba());
    no_accounts.transfer_rules = vec![TransferRule {
        pattern: "satispay".to_string(),
        account_ids: vec![],
    }];
    assert_eq!(
        field(
            statements
                .create_profile(&f.ctx, Origin::Api, no_accounts)
                .await
        ),
        "transfer_rules"
    );
}

#[tokio::test]
async fn another_households_profiles_and_accounts_are_out_of_reach() {
    let f = fixture().await;
    let statements = &f.test.app.statements;
    let created = statements
        .create_profile(
            &f.ctx,
            Origin::Api,
            input("Widiba", None, Mapping::widiba()),
        )
        .await
        .unwrap();
    let other = other_household(&f.test).await;
    assert!(matches!(
        statements.get_profile(&other, &created.id).await,
        Err(AppError::NotFound)
    ));
    assert!(statements.list_profiles(&other).await.unwrap().is_empty());
    let with_foreign_account = input("Mine", Some(&f.widiba), Mapping::widiba());
    let result = statements
        .create_profile(&other, Origin::Api, with_foreign_account)
        .await;
    assert!(
        matches!(result, Err(AppError::Invalid { .. })),
        "{result:?}"
    );
    let deleted = statements
        .delete_profile(&other, Origin::Api, &created.id)
        .await;
    assert!(matches!(deleted, Err(AppError::NotFound)));
}
