#![allow(clippy::unwrap_used)] // test helpers outside #[test] functions

mod common;

use common::*;
use saldo_app::accounts::AccountView;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::page::PageRequest;
use saldo_app::reconciliations::{NewReconciliation, ReconciliationView};
use saldo_app::transactions::{TransactionFilter, TransactionGroupView, input_from_view};
use saldo_core::GroupId;

struct Fixture {
    test: TestApp,
    ctx: HouseholdCtx,
    checking: AccountView,
    shop: AccountView,
    /// Salary on 1 Nov (2000.00), shop on 3 Nov (50.00), 5 Nov (30.00) and
    /// 20 Nov (10.00).
    groups: Vec<TransactionGroupView>,
}

async fn fixture() -> Fixture {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let salary = account(&test, &ctx, "Employer", "revenue", None).await;
    let shop = account(&test, &ctx, "Shop", "expense", None).await;
    let mut groups = Vec::new();
    for s in [
        split(1, &salary, &checking, "2000.00"),
        split(3, &checking, &shop, "50.00"),
        split(5, &checking, &shop, "30.00"),
        split(20, &checking, &shop, "10.00"),
    ] {
        let view = test
            .app
            .transactions
            .create(&ctx, Origin::Api, group(vec![s]))
            .await
            .unwrap();
        groups.push(view);
    }
    Fixture {
        test,
        ctx,
        checking,
        shop,
        groups,
    }
}

fn split_id(f: &Fixture, index: usize) -> String {
    f.groups[index].splits[0].id.clone()
}

fn statement(day: u32, balance: &str, split_ids: Vec<String>, book: bool) -> NewReconciliation {
    NewReconciliation {
        statement_date: date(day),
        statement_balance: balance.into(),
        split_ids,
        book_difference: book,
    }
}

async fn reconcile(f: &Fixture, input: NewReconciliation) -> Result<ReconciliationView, AppError> {
    f.test
        .app
        .reconciliations
        .create(&f.ctx, Origin::Api, account_id(&f.checking), input)
        .await
}

async fn reconciled(f: &Fixture, index: usize) -> bool {
    let id: GroupId = f.groups[index].id.parse().unwrap();
    let view = f.test.app.transactions.get(&f.ctx, id).await.unwrap();
    view.splits[0].reconciled
}

#[tokio::test]
async fn reconciles_the_selected_splits_and_books_a_difference() {
    let f = fixture().await;
    let service = &f.test.app.reconciliations;
    let draft = service
        .draft(&f.ctx, account_id(&f.checking), date(10))
        .await
        .unwrap();
    assert_eq!(draft.cleared_balance, "0.00");
    let amounts: Vec<_> = draft.splits.iter().map(|s| s.amount.as_str()).collect();
    assert_eq!(amounts, vec!["2000.00", "-50.00", "-30.00"]);

    let first = (0..3).map(|i| split_id(&f, i)).collect();
    let view = reconcile(&f, statement(10, "1920.00", first, false))
        .await
        .unwrap();
    assert_eq!(
        (view.difference.as_str(), view.split_ids.len()),
        ("0.00", 3)
    );
    assert!(reconciled(&f, 1).await);
    assert_eq!(
        last_event(&f.test, &f.ctx).await.event_type,
        "reconciliation.created"
    );
    let after = service
        .draft(&f.ctx, account_id(&f.checking), date(10))
        .await
        .unwrap();
    assert_eq!(
        (after.cleared_balance.as_str(), after.splits.len()),
        ("1920.00", 0)
    );
    assert_eq!(after.reconciled_through.as_deref(), Some("2026-11-10"));

    // The statement says 1905.00, the books 1910.00: 5.00 is missing.
    let late = vec![split_id(&f, 3)];
    let err = reconcile(&f, statement(30, "1905.00", late.clone(), false))
        .await
        .unwrap_err();
    assert!(
        matches!(err, AppError::Invalid { ref field, ref message } if field == "statement_balance" && message.contains("-5.00"))
    );
    let second = reconcile(&f, statement(30, "1905.00", late, true))
        .await
        .unwrap();
    assert_eq!(second.difference, "-5.00");
    assert_eq!(second.split_ids.len(), 2);
    let group_id: GroupId = second.difference_group_id.clone().unwrap().parse().unwrap();
    let booked = f.test.app.transactions.get(&f.ctx, group_id).await.unwrap();
    assert_eq!(
        (
            booked.splits[0].kind.as_str(),
            booked.splits[0].amount.as_str()
        ),
        ("reconciliation", "5.00")
    );
    assert_eq!(booked.splits[0].source_account_id, f.checking.id);

    // Only the latest can be undone; undoing removes the difference.
    let older = service.delete(&f.ctx, Origin::Api, &view.id).await;
    assert!(matches!(older, Err(AppError::Conflict(_))));
    service
        .delete(&f.ctx, Origin::Api, &second.id)
        .await
        .unwrap();
    assert!(!reconciled(&f, 3).await);
    let gone = f.test.app.transactions.get(&f.ctx, group_id).await;
    assert!(matches!(gone, Err(AppError::NotFound)));
    let history = service
        .list(
            &f.ctx,
            account_id(&f.checking),
            PageRequest::new(None, None).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(history.total, 1);
}

#[tokio::test]
async fn changing_what_the_bank_confirmed_unreconciles_a_split() {
    let f = fixture().await;
    let ids = (0..3).map(|i| split_id(&f, i)).collect();
    let view = reconcile(&f, statement(10, "1920.00", ids, false))
        .await
        .unwrap();
    let transactions = &f.test.app.transactions;

    // A new description keeps the split reconciled; a new amount does not.
    for (index, change) in [(2, "description"), (1, "amount")] {
        let id: GroupId = f.groups[index].id.parse().unwrap();
        let current = transactions.get(&f.ctx, id).await.unwrap();
        let mut input = input_from_view(&current).unwrap();
        match change {
            "description" => input.splits[0].description = "Groceries".into(),
            _ => input.splits[0].amount = "55.00".into(),
        }
        transactions
            .replace(&f.ctx, Origin::Api, id, input)
            .await
            .unwrap();
    }
    assert!(reconciled(&f, 2).await);
    assert!(!reconciled(&f, 1).await);
    let history = f
        .test
        .app
        .reconciliations
        .list(
            &f.ctx,
            account_id(&f.checking),
            PageRequest::new(None, None).unwrap(),
        )
        .await
        .unwrap();
    let now = &history.items[0];
    assert_eq!(now.id, view.id);
    assert!(!now.split_ids.contains(&split_id(&f, 1)));

    let open = transactions
        .list(
            &f.ctx,
            &TransactionFilter {
                reconciled: Some(false),
                ..Default::default()
            },
            PageRequest::new(None, None).unwrap(),
        )
        .await
        .unwrap();
    let mut open_ids: Vec<_> = open.items.iter().map(|g| g.id.clone()).collect();
    open_ids.sort();
    let mut expected = vec![f.groups[1].id.clone(), f.groups[3].id.clone()];
    expected.sort();
    assert_eq!(open_ids, expected);
}

#[tokio::test]
async fn the_statement_and_selection_are_checked() {
    let f = fixture().await;
    reconcile(
        &f,
        statement(
            10,
            "1920.00",
            (0..3).map(|i| split_id(&f, i)).collect(),
            false,
        ),
    )
    .await
    .unwrap();
    let cases = [
        // Before the last reconciliation.
        ("statement_date", statement(9, "1920.00", vec![], true)),
        // Already reconciled.
        (
            "split_ids",
            statement(30, "1910.00", vec![split_id(&f, 1)], true),
        ),
        // After the statement date.
        (
            "split_ids",
            statement(19, "1910.00", vec![split_id(&f, 3)], true),
        ),
        ("statement_balance", statement(30, "abc", vec![], true)),
    ];
    for (field, input) in cases {
        let err = reconcile(&f, input).await.unwrap_err();
        assert!(
            matches!(err, AppError::Invalid { field: ref got, .. } if got == field),
            "{field}: {err:?}"
        );
    }
    let expense = f
        .test
        .app
        .reconciliations
        .create(
            &f.ctx,
            Origin::Api,
            account_id(&f.shop),
            statement(30, "0", vec![], true),
        )
        .await;
    assert!(matches!(expense, Err(AppError::Invalid { ref field, .. }) if field == "account_id"));
}
