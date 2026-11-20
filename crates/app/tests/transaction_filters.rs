#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

//! The filters added in Phase 10: amount range, "no category", "no budget"
//! and "has attachments", and that the page total always matches the IDs.

mod common;

use common::*;
use saldo_app::attachments::NewAttachment;
use saldo_app::budgets::NewBudget;
use saldo_app::categories::NewCategory;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::page::PageRequest;
use saldo_app::transactions::TransactionFilter;

/// Five groups, one split each, described by what makes them different.
async fn seed(test: &TestApp, ctx: &HouseholdCtx) -> (String, String) {
    let checking = account(test, ctx, "Checking", "asset", None).await;
    let food = account(test, ctx, "Food", "expense", None).await;
    let salary = account(test, ctx, "Salary", "revenue", None).await;
    let savings = account(test, ctx, "Savings", "asset", None).await;
    let category = test
        .app
        .categories
        .create(
            ctx,
            Origin::Api,
            NewCategory {
                name: "Groceries".into(),
                notes: None,
                color: None,
            },
        )
        .await
        .unwrap();
    let budget = test
        .app
        .budgets
        .create(
            ctx,
            Origin::Api,
            NewBudget {
                name: "Food".into(),
                ..NewBudget::default()
            },
        )
        .await
        .unwrap();

    let mut categorised = split(1, &checking, &food, "12.50");
    categorised.description = "categorised".into();
    categorised.category_id = Some(category.id.parse().unwrap());
    categorised.budget_id = Some(budget.id.parse().unwrap());
    let mut plain = split(2, &checking, &food, "80");
    plain.description = "plain".into();
    let mut income = split(3, &salary, &checking, "2000");
    income.description = "income".into();
    let mut transfer = split(4, &checking, &savings, "50");
    transfer.description = "transfer".into();
    let mut receipt = split(5, &checking, &food, "5");
    receipt.description = "receipt".into();
    receipt.budget_id = Some(budget.id.parse().unwrap());

    let mut receipt_split = String::new();
    for input in [categorised, plain, income, transfer, receipt] {
        let view = test
            .app
            .transactions
            .create(ctx, Origin::Api, group(vec![input]))
            .await
            .unwrap();
        receipt_split = view.splits[0].id.clone();
    }
    (receipt_split, category.id)
}

async fn descriptions(
    test: &TestApp,
    ctx: &HouseholdCtx,
    filter: TransactionFilter,
) -> Vec<String> {
    let page = test
        .app
        .transactions
        .list(ctx, &filter, PageRequest::new(Some(1), Some(200)).unwrap())
        .await
        .unwrap();
    assert_eq!(
        page.total,
        i64::try_from(page.items.len()).unwrap(),
        "total must count the same groups as the page"
    );
    let mut names: Vec<_> = page
        .items
        .iter()
        .map(|g| g.splits[0].description.clone())
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn amount_range_no_category_no_budget_and_attachments() {
    let dir = std::env::temp_dir().join(format!("saldo-filters-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (mut test, setup) = with_household().await;
    test.app = test.app.with_data_dir(dir);
    let ctx = setup.ctx;
    let (receipt_split, _) = seed(&test, &ctx).await;
    test.app
        .attachments
        .upload(
            &ctx,
            Origin::Api,
            NewAttachment {
                attachable_type: "split".into(),
                attachable_id: receipt_split,
                filename: "receipt.pdf".into(),
                title: None,
                notes: None,
                mime_type: "application/pdf".into(),
                bytes: b"%PDF-1".to_vec(),
            },
        )
        .await
        .unwrap();

    let range = |min: Option<&str>, max: Option<&str>| TransactionFilter {
        amount_min: min.map(str::to_string),
        amount_max: max.map(str::to_string),
        ..Default::default()
    };
    assert_eq!(
        descriptions(&test, &ctx, range(Some("12.50"), Some("80"))).await,
        ["categorised", "plain", "transfer"]
    );
    assert_eq!(
        descriptions(&test, &ctx, range(Some("100"), None)).await,
        ["income"]
    );
    assert_eq!(
        descriptions(&test, &ctx, range(None, Some("5"))).await,
        ["receipt"]
    );
    // Splits in another currency never match an amount range in EUR.
    let other = TransactionFilter {
        currency: Some("USD".into()),
        ..range(Some("1"), None)
    };
    assert!(descriptions(&test, &ctx, other).await.is_empty());

    let no_category = TransactionFilter {
        no_category: true,
        ..Default::default()
    };
    // Transfers are left out: they rarely carry a category.
    assert_eq!(
        descriptions(&test, &ctx, no_category).await,
        ["income", "plain", "receipt"]
    );
    let no_budget = TransactionFilter {
        no_budget: true,
        ..Default::default()
    };
    // Only withdrawals take a budget.
    assert_eq!(descriptions(&test, &ctx, no_budget).await, ["plain"]);

    let with_files = TransactionFilter {
        has_attachments: Some(true),
        ..Default::default()
    };
    assert_eq!(descriptions(&test, &ctx, with_files).await, ["receipt"]);
    let without_files = TransactionFilter {
        has_attachments: Some(false),
        no_budget: true,
        ..Default::default()
    };
    assert_eq!(descriptions(&test, &ctx, without_files).await, ["plain"]);
}

#[tokio::test]
async fn invalid_amount_ranges_are_rejected() {
    let (test, setup) = with_household().await;
    let field_of = |result: Result<_, AppError>| match result {
        Err(AppError::Invalid { field, .. }) => field,
        other => panic!("expected an invalid field, got {other:?}"),
    };
    let list = |filter: TransactionFilter| {
        let app = test.app.clone();
        async move {
            app.transactions
                .list(&setup.ctx, &filter, PageRequest::default())
                .await
        }
    };
    let backwards = TransactionFilter {
        amount_min: Some("10".into()),
        amount_max: Some("5".into()),
        ..Default::default()
    };
    assert_eq!(field_of(list(backwards).await), "amount_min");
    let garbage = TransactionFilter {
        amount_max: Some("ten".into()),
        ..Default::default()
    };
    assert_eq!(field_of(list(garbage).await), "amount_max");
    let negative = TransactionFilter {
        amount_min: Some("-1".into()),
        ..Default::default()
    };
    assert_eq!(field_of(list(negative).await), "amount_min");
    let unknown = TransactionFilter {
        amount_min: Some("1".into()),
        currency: Some("XXX".into()),
        ..Default::default()
    };
    assert_eq!(field_of(list(unknown).await), "currency");
}
