#![allow(clippy::unwrap_used)] // test helpers outside #[test] functions

mod common;

use common::*;
use saldo_app::accounts::AccountView;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::page::PageRequest;
use saldo_app::piggy_banks::{NewPiggyBank, NewPiggyBankEvent, PiggyBankUpdate, PiggyBankView};
use saldo_core::{PiggyBankId, SplitId};

struct Fixture {
    test: TestApp,
    ctx: HouseholdCtx,
    savings: AccountView,
    checking: AccountView,
    salary: AccountView,
}

/// Savings holds 500.00, checking 1000.00.
async fn fixture() -> Fixture {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let savings = account(&test, &ctx, "Savings", "asset", None).await;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let salary = account(&test, &ctx, "Salary", "revenue", None).await;
    let tx = &test.app.transactions;
    tx.create(
        &ctx,
        Origin::Api,
        group(vec![split(1, &salary, &savings, "500.00")]),
    )
    .await
    .unwrap();
    tx.create(
        &ctx,
        Origin::Api,
        group(vec![split(1, &salary, &checking, "1000.00")]),
    )
    .await
    .unwrap();
    Fixture {
        test,
        ctx,
        savings,
        checking,
        salary,
    }
}

fn piggy(name: &str, target: Option<&str>, accounts: &[&AccountView]) -> NewPiggyBank {
    NewPiggyBank {
        name: name.to_string(),
        target_amount: target.map(str::to_string),
        account_ids: accounts.iter().map(|a| account_id(a)).collect(),
        ..NewPiggyBank::default()
    }
}

fn id(view: &PiggyBankView) -> PiggyBankId {
    view.id.parse().unwrap()
}

fn money(amount: &str) -> NewPiggyBankEvent {
    NewPiggyBankEvent {
        account_id: None,
        amount: amount.to_string(),
        date: date(13),
        split_id: None,
    }
}

async fn create(f: &Fixture, input: NewPiggyBank) -> PiggyBankView {
    f.test
        .app
        .piggy_banks
        .create(&f.ctx, Origin::Api, input)
        .await
        .unwrap()
}

fn invalid_field(error: AppError) -> String {
    match error {
        AppError::Invalid { field, .. } => field,
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[tokio::test]
async fn create_links_accounts_and_appends_an_event() {
    let f = fixture().await;
    let view = create(&f, piggy("Holiday", Some("300.00"), &[&f.savings])).await;

    assert_eq!(view.currency, "EUR");
    assert_eq!(view.target_amount.as_deref(), Some("300.00"));
    assert_eq!(view.saved_amount, "0.00");
    assert_eq!(view.left_to_save.as_deref(), Some("300.00"));
    assert_eq!(view.accounts.len(), 1);
    assert_eq!(view.accounts[0].account_id, f.savings.id);
    let event = last_event(&f.test, &f.ctx).await;
    assert_eq!(event.event_type, "piggy_bank.created");
    assert_eq!(event.payload["data"]["name"], "Holiday");
}

#[tokio::test]
async fn create_rejects_bad_input() {
    let f = fixture().await;
    let service = &f.test.app.piggy_banks;
    let cases = [
        (piggy(" ", None, &[&f.savings]), "name"),
        (piggy("A", Some("0"), &[&f.savings]), "target_amount"),
        (piggy("A", Some("1.234"), &[&f.savings]), "target_amount"),
        (piggy("A", None, &[]), "account_ids"),
        (piggy("A", None, &[&f.salary]), "account_ids"),
    ];
    for (input, field) in cases {
        let error = service
            .create(&f.ctx, Origin::Api, input)
            .await
            .unwrap_err();
        assert_eq!(invalid_field(error), field);
    }
    let usd = account(&f.test, &f.ctx, "Dollars", "asset", Some("USD")).await;
    let error = service
        .create(&f.ctx, Origin::Api, piggy("A", None, &[&usd]))
        .await
        .unwrap_err();
    assert_eq!(invalid_field(error), "account_ids");
    let backwards = NewPiggyBank {
        start_date: Some(date(20)),
        target_date: Some(date(10)),
        ..piggy("A", None, &[&f.savings])
    };
    let error = service
        .create(&f.ctx, Origin::Api, backwards)
        .await
        .unwrap_err();
    assert_eq!(invalid_field(error), "target_date");
    assert_eq!(count(&f.test, "piggy_banks").await, 0);
}

#[tokio::test]
async fn duplicate_names_conflict() {
    let f = fixture().await;
    create(&f, piggy("Holiday", None, &[&f.savings])).await;
    let error = f
        .test
        .app
        .piggy_banks
        .create(&f.ctx, Origin::Api, piggy("Holiday", None, &[&f.savings]))
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::Conflict(_)));
}

#[tokio::test]
async fn saved_amount_is_the_sum_of_events() {
    let f = fixture().await;
    let view = create(&f, piggy("Holiday", None, &[&f.savings])).await;
    let service = &f.test.app.piggy_banks;
    service
        .add_event(&f.ctx, Origin::Api, id(&view), money("100.00"))
        .await
        .unwrap();
    let removed = service
        .add_event(&f.ctx, Origin::Api, id(&view), money("-30.50"))
        .await
        .unwrap();
    assert_eq!(removed.amount, "-30.50");
    assert_eq!(removed.account_id, f.savings.id);

    let view = service.get(&f.ctx, id(&view)).await.unwrap();
    assert_eq!(view.saved_amount, "69.50");
    assert_eq!(view.left_to_save, None);
    assert_eq!(view.accounts[0].saved_amount, "69.50");
    let events = service
        .list_events(&f.ctx, id(&view), PageRequest::default())
        .await
        .unwrap();
    assert_eq!(events.total, 2);
    let last = last_event(&f.test, &f.ctx).await;
    assert_eq!(last.event_type, "piggy_bank.updated");
    assert_eq!(last.payload["data"]["saved_amount"], "69.50");
}

#[tokio::test]
async fn saving_cannot_exceed_the_account_balance() {
    let f = fixture().await;
    let view = create(&f, piggy("Car", None, &[&f.savings])).await;
    let service = &f.test.app.piggy_banks;
    service
        .add_event(&f.ctx, Origin::Api, id(&view), money("450.00"))
        .await
        .unwrap();
    let error = service
        .add_event(&f.ctx, Origin::Api, id(&view), money("50.01"))
        .await
        .unwrap_err();
    match error {
        AppError::Invalid { field, message } => {
            assert_eq!(field, "amount");
            assert!(message.contains("50.00 EUR"), "{message}");
        }
        other => panic!("{other:?}"),
    }
    let error = service
        .add_event(&f.ctx, Origin::Api, id(&view), money("-450.01"))
        .await
        .unwrap_err();
    assert_eq!(invalid_field(error), "amount");
    let error = service
        .add_event(&f.ctx, Origin::Api, id(&view), money("0"))
        .await
        .unwrap_err();
    assert_eq!(invalid_field(error), "amount");
}

#[tokio::test]
async fn reaching_the_target_appends_target_reached() {
    let f = fixture().await;
    let view = create(&f, piggy("Bike", Some("200.00"), &[&f.savings])).await;
    let service = &f.test.app.piggy_banks;
    service
        .add_event(&f.ctx, Origin::Api, id(&view), money("150.00"))
        .await
        .unwrap();
    service
        .add_event(&f.ctx, Origin::Api, id(&view), money("50.00"))
        .await
        .unwrap();
    let types: Vec<_> = events(&f.test, &f.ctx)
        .await
        .into_iter()
        .map(|e| e.event_type)
        .filter(|t| t.starts_with("piggy_bank."))
        .collect();
    assert_eq!(
        types,
        [
            "piggy_bank.created",
            "piggy_bank.updated",
            "piggy_bank.updated",
            "piggy_bank.target_reached"
        ]
    );
    let error = service
        .add_event(&f.ctx, Origin::Api, id(&view), money("0.01"))
        .await
        .unwrap_err();
    assert_eq!(invalid_field(error), "amount");
    let view = service.get(&f.ctx, id(&view)).await.unwrap();
    assert_eq!(view.left_to_save.as_deref(), Some("0.00"));
}

#[tokio::test]
async fn several_accounts_need_an_explicit_account() {
    let f = fixture().await;
    let view = create(&f, piggy("House", None, &[&f.savings, &f.checking])).await;
    let service = &f.test.app.piggy_banks;
    let error = service
        .add_event(&f.ctx, Origin::Api, id(&view), money("10.00"))
        .await
        .unwrap_err();
    assert_eq!(invalid_field(error), "account_id");
    let other = account(&f.test, &f.ctx, "Other", "asset", None).await;
    let unlinked = NewPiggyBankEvent {
        account_id: Some(account_id(&other)),
        ..money("10.00")
    };
    let error = service
        .add_event(&f.ctx, Origin::Api, id(&view), unlinked)
        .await
        .unwrap_err();
    assert_eq!(invalid_field(error), "account_id");
    let on_checking = NewPiggyBankEvent {
        account_id: Some(account_id(&f.checking)),
        ..money("700.00")
    };
    service
        .add_event(&f.ctx, Origin::Api, id(&view), on_checking)
        .await
        .unwrap();
    let view = service.get(&f.ctx, id(&view)).await.unwrap();
    let checking = view
        .accounts
        .iter()
        .find(|a| a.account_id == f.checking.id)
        .unwrap();
    assert_eq!(checking.saved_amount, "700.00");
}

#[tokio::test]
async fn events_may_point_at_a_transfer_split_of_the_account() {
    let f = fixture().await;
    let view = create(&f, piggy("Trip", None, &[&f.savings])).await;
    let tx = &f.test.app.transactions;
    let transfer = tx
        .create(
            &f.ctx,
            Origin::Api,
            group(vec![split(2, &f.checking, &f.savings, "40.00")]),
        )
        .await
        .unwrap();
    let deposit = tx
        .create(
            &f.ctx,
            Origin::Api,
            group(vec![split(2, &f.salary, &f.savings, "5.00")]),
        )
        .await
        .unwrap();
    let split_id = |g: &saldo_app::transactions::TransactionGroupView| -> SplitId {
        g.splits[0].id.parse().unwrap()
    };
    let service = &f.test.app.piggy_banks;
    let linked = NewPiggyBankEvent {
        split_id: Some(split_id(&transfer)),
        ..money("40.00")
    };
    let event = service
        .add_event(&f.ctx, Origin::Api, id(&view), linked)
        .await
        .unwrap();
    assert_eq!(
        event.split_id.as_deref(),
        Some(transfer.splits[0].id.as_str())
    );
    let not_transfer = NewPiggyBankEvent {
        split_id: Some(split_id(&deposit)),
        ..money("5.00")
    };
    let error = service
        .add_event(&f.ctx, Origin::Api, id(&view), not_transfer)
        .await
        .unwrap_err();
    assert_eq!(invalid_field(error), "split_id");
    let unknown = NewPiggyBankEvent {
        split_id: Some(SplitId(uuid::Uuid::from_u128(7))),
        ..money("5.00")
    };
    let error = service
        .add_event(&f.ctx, Origin::Api, id(&view), unknown)
        .await
        .unwrap_err();
    assert_eq!(invalid_field(error), "split_id");
}

#[tokio::test]
async fn imports_skip_balance_and_target_checks() {
    let f = fixture().await;
    let view = create(&f, piggy("Old", Some("10.00"), &[&f.savings])).await;
    let event = f
        .test
        .app
        .piggy_banks
        .add_event(&f.ctx, Origin::Import, id(&view), money("9999.00"))
        .await
        .unwrap();
    assert_eq!(event.amount, "9999.00");
}

#[tokio::test]
async fn update_changes_fields_and_refuses_to_unlink_saved_money() {
    let f = fixture().await;
    let view = create(&f, piggy("Holiday", Some("300.00"), &[&f.savings])).await;
    let service = &f.test.app.piggy_banks;
    service
        .add_event(&f.ctx, Origin::Api, id(&view), money("10.00"))
        .await
        .unwrap();
    let update = PiggyBankUpdate {
        name: Some("Summer".to_string()),
        target_amount: Some(None),
        notes: Some(Some("beach".to_string())),
        target_date: Some(Some(date(30))),
        account_ids: Some(vec![account_id(&f.savings), account_id(&f.checking)]),
        ..PiggyBankUpdate::default()
    };
    let updated = service
        .update(&f.ctx, Origin::Api, id(&view), update)
        .await
        .unwrap();
    assert_eq!(updated.name, "Summer");
    assert_eq!(updated.target_amount, None);
    assert_eq!(updated.notes.as_deref(), Some("beach"));
    assert_eq!(updated.target_date.as_deref(), Some("2026-11-30"));
    assert_eq!(updated.accounts.len(), 2);
    assert_eq!(updated.saved_amount, "10.00");
    assert_eq!(
        last_event(&f.test, &f.ctx).await.event_type,
        "piggy_bank.updated"
    );

    let drop_savings = PiggyBankUpdate {
        account_ids: Some(vec![account_id(&f.checking)]),
        ..PiggyBankUpdate::default()
    };
    let error = service
        .update(&f.ctx, Origin::Api, id(&view), drop_savings)
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::Conflict(_)));
    let drop_checking = PiggyBankUpdate {
        account_ids: Some(vec![account_id(&f.savings)]),
        ..PiggyBankUpdate::default()
    };
    let updated = service
        .update(&f.ctx, Origin::Api, id(&view), drop_checking)
        .await
        .unwrap();
    assert_eq!(updated.accounts.len(), 1);
}

#[tokio::test]
async fn delete_removes_events_and_lists_are_per_household() {
    let f = fixture().await;
    let view = create(&f, piggy("Holiday", None, &[&f.savings])).await;
    create(&f, piggy("Archived", None, &[&f.savings])).await;
    let service = &f.test.app.piggy_banks;
    service
        .add_event(&f.ctx, Origin::Api, id(&view), money("10.00"))
        .await
        .unwrap();
    let other = other_household(&f.test).await;
    assert!(matches!(
        service.get(&other, id(&view)).await.unwrap_err(),
        AppError::NotFound
    ));
    assert_eq!(
        service
            .list(&other, None, PageRequest::default())
            .await
            .unwrap()
            .total,
        0
    );
    let page = service
        .list(&f.ctx, None, PageRequest::default())
        .await
        .unwrap();
    let names: Vec<_> = page.items.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Archived", "Holiday"]);
    assert_eq!(
        service
            .list(&f.ctx, Some(false), PageRequest::default())
            .await
            .unwrap()
            .total,
        0
    );

    service
        .delete(&f.ctx, Origin::Api, id(&view))
        .await
        .unwrap();
    assert_eq!(count(&f.test, "piggy_bank_events").await, 0);
    let event = last_event(&f.test, &f.ctx).await;
    assert_eq!(event.event_type, "piggy_bank.deleted");
    assert_eq!(event.payload["data"]["saved_amount"], "10.00");
    assert!(matches!(
        service
            .delete(&f.ctx, Origin::Api, id(&view))
            .await
            .unwrap_err(),
        AppError::NotFound
    ));
}

#[tokio::test]
async fn the_linked_side_of_a_split_is_found() {
    let f = fixture().await;
    let view = create(&f, piggy("Trip", None, &[&f.savings])).await;
    let transfer = f
        .test
        .app
        .transactions
        .create(
            &f.ctx,
            Origin::Api,
            group(vec![split(2, &f.checking, &f.savings, "4.00")]),
        )
        .await
        .unwrap();
    let deposit = f
        .test
        .app
        .transactions
        .create(
            &f.ctx,
            Origin::Api,
            group(vec![split(2, &f.salary, &f.checking, "4.00")]),
        )
        .await
        .unwrap();
    let service = &f.test.app.piggy_banks;
    let mut conn = f.test.app.pool.acquire().await.unwrap();
    let side = |g: &saldo_app::transactions::TransactionGroupView| -> SplitId {
        g.splits[0].id.parse().unwrap()
    };
    let found = service
        .linked_side_of_split_in(&mut conn, &f.ctx, id(&view), side(&transfer))
        .await
        .unwrap();
    assert_eq!(found, Some(account_id(&f.savings)));
    let none = service
        .linked_side_of_split_in(&mut conn, &f.ctx, id(&view), side(&deposit))
        .await
        .unwrap();
    assert_eq!(none, None);
    let unknown = service
        .linked_side_of_split_in(
            &mut conn,
            &f.ctx,
            id(&view),
            SplitId(uuid::Uuid::from_u128(1)),
        )
        .await
        .unwrap();
    assert_eq!(unknown, None);
}
