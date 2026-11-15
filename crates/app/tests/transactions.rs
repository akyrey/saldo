mod common;

use common::*;
use saldo_app::categories::NewCategory;
use saldo_app::ctx::Origin;
use saldo_app::error::AppError;
use saldo_app::page::PageRequest;
use saldo_app::scope::Scope;
use saldo_app::tags::NewTag;
use saldo_app::transactions::{TransactionFilter, TransactionInput};
use saldo_core::{BudgetId, GroupId};

async fn codes_of(
    test: &TestApp,
    ctx: &saldo_app::ctx::HouseholdCtx,
    input: TransactionInput,
) -> Vec<&'static str> {
    match test.app.transactions.create(ctx, Origin::Api, input).await {
        Err(AppError::Validation(errors)) => errors.iter().map(|e| e.code()).collect(),
        other => panic!("expected validation errors, got {other:?}"),
    }
}

#[tokio::test]
async fn group_with_different_sources_and_destinations() {
    let (test, setup) = with_household().await;
    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let card = account(&test, &setup.ctx, "Card", "liability", None).await;
    let food = account(&test, &setup.ctx, "Food", "expense", None).await;
    let input = group(vec![
        split(1, &checking, &food, "42.50"),
        split(2, &card, &checking, "100"),
    ]);

    let view = test
        .app
        .transactions
        .create(&setup.ctx, Origin::Api, input)
        .await
        .unwrap();

    assert_eq!(view.title.as_deref(), Some("Test"));
    assert_eq!(
        view.created_by_user_id.as_deref(),
        Some(setup.user.id.as_str())
    );
    let kinds: Vec<_> = view.splits.iter().map(|s| s.kind.as_str()).collect();
    assert_eq!(kinds, ["withdrawal", "transfer"]);
    assert_eq!(
        (
            view.splits[0].amount.as_str(),
            view.splits[0].currency.as_str()
        ),
        ("42.50", "EUR")
    );
    assert_eq!(view.splits[1].amount, "100.00");
    assert_eq!(view.splits[1].position, 1);

    let event = last_event(&test, &setup.ctx).await;
    assert_eq!(event.event_type, "transaction.created");
    assert_eq!(event.payload["data"], serde_json::to_value(&view).unwrap());
    let id: GroupId = view.id.parse().unwrap();
    assert_eq!(
        test.app.transactions.get(&setup.ctx, id).await.unwrap(),
        view
    );
}

#[tokio::test]
async fn failed_validation_writes_no_rows_and_no_event() {
    let (test, setup) = with_household().await;
    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let food = account(&test, &setup.ctx, "Food", "expense", None).await;
    let events_before = count(&test, "events").await;

    let input = group(vec![
        split(1, &checking, &food, "5"),
        split(1, &food, &checking, "0"),
        split(1, &checking, &checking, "1"),
    ]);
    let codes = codes_of(&test, &setup.ctx, input).await;

    assert_eq!(
        codes,
        [
            "amount_not_positive",
            "invalid_account_pair",
            "same_account"
        ]
    );
    assert_eq!(count(&test, "transaction_groups").await, 0);
    assert_eq!(count(&test, "splits").await, 0);
    assert_eq!(count(&test, "events").await, events_before);
    assert_eq!(
        codes_of(&test, &setup.ctx, group(vec![])).await,
        ["empty_group"]
    );
}

#[tokio::test]
async fn bad_amounts_name_the_field() {
    let (test, setup) = with_household().await;
    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let food = account(&test, &setup.ctx, "Food", "expense", None).await;
    let field_of = |input| {
        let app = test.app.clone();
        let ctx = setup.ctx;
        async move {
            match app.transactions.create(&ctx, Origin::Api, input).await {
                Err(AppError::Invalid { field, .. }) => field,
                other => panic!("expected invalid field, got {other:?}"),
            }
        }
    };
    let mut too_precise = split(1, &checking, &food, "1.234");
    assert_eq!(
        field_of(group(vec![
            split(1, &checking, &food, "5"),
            too_precise.clone()
        ]))
        .await,
        "splits[1].amount"
    );
    too_precise.amount = "1".into();
    too_precise.foreign_amount = Some("2".into());
    assert_eq!(
        field_of(group(vec![too_precise.clone()])).await,
        "splits[0].foreign_currency"
    );
    too_precise.foreign_amount = None;
    too_precise.foreign_currency = Some("USD".into());
    assert_eq!(
        field_of(group(vec![too_precise.clone()])).await,
        "splits[0].foreign_amount"
    );
    too_precise.foreign_currency = None;
    too_precise.currency = Some("XXX".into());
    assert_eq!(
        field_of(group(vec![too_precise.clone()])).await,
        "splits[0].currency"
    );
    too_precise.currency = None;
    too_precise.description = " ".into();
    assert_eq!(
        field_of(group(vec![too_precise])).await,
        "splits[0].description"
    );
}

#[tokio::test]
async fn currency_defaults_to_the_asset_side_and_is_checked() {
    let (test, setup) = with_household().await;
    let wallet = account(&test, &setup.ctx, "Wallet", "asset", Some("USD")).await;
    let salary = account(&test, &setup.ctx, "Salary", "revenue", None).await;
    let view = test
        .app
        .transactions
        .create(
            &setup.ctx,
            Origin::Api,
            group(vec![split(1, &salary, &wallet, "10")]),
        )
        .await
        .unwrap();
    assert_eq!(view.splits[0].currency, "USD");

    let mut wrong = split(1, &salary, &wallet, "10");
    wrong.currency = Some("EUR".into());
    assert_eq!(
        codes_of(&test, &setup.ctx, group(vec![wrong])).await,
        ["currency_mismatch"]
    );
}

#[tokio::test]
async fn foreign_currency_transfer_moves_both_amounts() {
    let (test, setup) = with_household().await;
    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let wallet = account(&test, &setup.ctx, "Wallet", "asset", Some("USD")).await;
    assert_eq!(
        codes_of(
            &test,
            &setup.ctx,
            group(vec![split(1, &checking, &wallet, "100")])
        )
        .await,
        ["foreign_amount_required"]
    );

    let mut transfer = split(1, &checking, &wallet, "100");
    transfer.foreign_amount = Some("108.20".into());
    transfer.foreign_currency = Some("USD".into());
    let view = test
        .app
        .transactions
        .create(&setup.ctx, Origin::Api, group(vec![transfer]))
        .await
        .unwrap();
    assert_eq!(view.splits[0].foreign_amount.as_deref(), Some("108.20"));

    let usd = test
        .app
        .accounts
        .balance(&setup.ctx, account_id(&wallet), None)
        .await
        .unwrap();
    let eur = test
        .app
        .accounts
        .balance(&setup.ctx, account_id(&checking), None)
        .await
        .unwrap();
    assert_eq!(
        (usd.balance.as_str(), eur.balance.as_str()),
        ("108.20", "-100.00")
    );
}

#[tokio::test]
async fn categories_and_tags_must_exist_in_the_household() {
    let (test, setup) = with_household().await;
    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let food = account(&test, &setup.ctx, "Food", "expense", None).await;
    let category = test
        .app
        .categories
        .create(
            &setup.ctx,
            Origin::Api,
            NewCategory {
                name: "Groceries".into(),
                notes: None,
                color: None,
            },
        )
        .await
        .unwrap();
    let tag = test
        .app
        .tags
        .create(
            &setup.ctx,
            Origin::Api,
            NewTag {
                name: "weekly".into(),
                description: None,
            },
        )
        .await
        .unwrap();

    let mut good = split(1, &checking, &food, "5");
    good.category_id = Some(category.id.parse().unwrap());
    good.tag_ids = vec![tag.id.parse().unwrap(), tag.id.parse().unwrap()];
    good.meta.insert("sepa_ct_id".into(), "ref-1".into());
    let view = test
        .app
        .transactions
        .create(&setup.ctx, Origin::Api, group(vec![good]))
        .await
        .unwrap();
    assert_eq!(view.splits[0].category_id.as_ref(), Some(&category.id));
    assert_eq!(view.splits[0].tag_ids, std::slice::from_ref(&tag.id));
    assert_eq!(view.splits[0].meta["sepa_ct_id"], "ref-1");

    let mut unknown = split(1, &checking, &food, "5");
    unknown.category_id = Some(saldo_core::CategoryId(uuid::Uuid::from_u128(5)));
    unknown.budget_id = Some(BudgetId(uuid::Uuid::from_u128(6)));
    assert_eq!(
        codes_of(&test, &setup.ctx, group(vec![unknown])).await,
        ["unknown_reference", "unknown_reference"]
    );
}

#[tokio::test]
async fn replace_swaps_the_splits_and_appends_updated() {
    let (test, setup) = with_household().await;
    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let food = account(&test, &setup.ctx, "Food", "expense", None).await;
    let fuel = account(&test, &setup.ctx, "Fuel", "expense", None).await;
    let created = test
        .app
        .transactions
        .create(
            &setup.ctx,
            Origin::Api,
            group(vec![split(1, &checking, &food, "5")]),
        )
        .await
        .unwrap();
    let id: GroupId = created.id.parse().unwrap();

    let input = TransactionInput {
        title: Some("  ".into()),
        splits: vec![
            split(2, &checking, &fuel, "60"),
            split(2, &checking, &food, "7"),
        ],
    };
    let view = test
        .app
        .transactions
        .replace(&setup.ctx, Origin::Api, id, input)
        .await
        .unwrap();
    assert_eq!(view.title, None);
    assert_eq!(view.splits.len(), 2);
    assert_ne!(view.splits[0].id, created.splits[0].id);
    assert_eq!(count(&test, "splits").await, 2);
    assert_eq!(
        last_event(&test, &setup.ctx).await.event_type,
        "transaction.updated"
    );

    let invalid = test
        .app
        .transactions
        .replace(
            &setup.ctx,
            Origin::Api,
            id,
            group(vec![split(3, &food, &fuel, "1")]),
        )
        .await;
    assert!(matches!(invalid, Err(AppError::Validation(_))));
    assert_eq!(
        test.app.transactions.get(&setup.ctx, id).await.unwrap(),
        view
    );

    let missing = GroupId(uuid::Uuid::from_u128(9));
    let result = test
        .app
        .transactions
        .replace(
            &setup.ctx,
            Origin::Api,
            missing,
            group(vec![split(1, &checking, &food, "1")]),
        )
        .await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn delete_removes_the_group_and_records_its_last_state() {
    let (test, setup) = with_household().await;
    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let food = account(&test, &setup.ctx, "Food", "expense", None).await;
    let created = test
        .app
        .transactions
        .create(
            &setup.ctx,
            Origin::Api,
            group(vec![split(1, &checking, &food, "5")]),
        )
        .await
        .unwrap();
    let id: GroupId = created.id.parse().unwrap();

    test.app
        .transactions
        .delete(&setup.ctx, Origin::Api, id)
        .await
        .unwrap();

    assert_eq!(count(&test, "splits").await, 0);
    let event = last_event(&test, &setup.ctx).await;
    assert_eq!(event.event_type, "transaction.deleted");
    assert_eq!(
        event.payload["data"],
        serde_json::to_value(&created).unwrap()
    );
    assert!(matches!(
        test.app.transactions.get(&setup.ctx, id).await,
        Err(AppError::NotFound)
    ));
    assert!(matches!(
        test.app
            .transactions
            .delete(&setup.ctx, Origin::Api, id)
            .await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn lists_with_filters_newest_first() {
    let (test, setup) = with_household().await;
    let member = add_member(&test, &setup.ctx, "partner@example.com").await;
    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let mut theirs_input = saldo_app::accounts::NewAccount {
        name: "Theirs".into(),
        account_type: "asset".into(),
        ..Default::default()
    };
    theirs_input.owner_user_id = Some(member.user_id);
    let theirs = test
        .app
        .accounts
        .create(&setup.ctx, Origin::Api, theirs_input)
        .await
        .unwrap();
    let food = account(&test, &setup.ctx, "Food", "expense", None).await;
    let salary = account(&test, &setup.ctx, "Salary", "revenue", None).await;
    let tag = test
        .app
        .tags
        .create(
            &setup.ctx,
            Origin::Api,
            NewTag {
                name: "t".into(),
                description: None,
            },
        )
        .await
        .unwrap();
    let mut tagged = split(10, &checking, &food, "3");
    tagged.description = "Bakery".into();
    tagged.tag_ids = vec![tag.id.parse().unwrap()];
    for input in [
        group(vec![split(1, &salary, &checking, "100")]),
        group(vec![tagged]),
        group(vec![split(5, &theirs, &food, "8")]),
    ] {
        test.app
            .transactions
            .create(&setup.ctx, Origin::Api, input)
            .await
            .unwrap();
    }

    let titles = |filter: TransactionFilter| {
        let app = test.app.clone();
        let ctx = setup.ctx;
        async move {
            let page = app
                .transactions
                .list(&ctx, &filter, PageRequest::default())
                .await
                .unwrap();
            page.items
                .iter()
                .map(|g| g.splits[0].date.clone())
                .collect::<Vec<_>>()
        }
    };
    assert_eq!(
        titles(TransactionFilter::default()).await,
        ["2026-11-10", "2026-11-05", "2026-11-01"]
    );
    assert_eq!(
        titles(TransactionFilter {
            from: Some(date(2)),
            to: Some(date(9)),
            ..Default::default()
        })
        .await,
        ["2026-11-05"]
    );
    assert_eq!(
        titles(TransactionFilter {
            kind: Some("deposit".into()),
            ..Default::default()
        })
        .await,
        ["2026-11-01"]
    );
    assert_eq!(
        titles(TransactionFilter {
            q: Some("aker".into()),
            ..Default::default()
        })
        .await,
        ["2026-11-10"]
    );
    assert_eq!(
        titles(TransactionFilter {
            tag_id: Some(tag.id.parse().unwrap()),
            ..Default::default()
        })
        .await,
        ["2026-11-10"]
    );
    assert_eq!(
        titles(TransactionFilter {
            account_id: Some(account_id(&salary)),
            ..Default::default()
        })
        .await,
        ["2026-11-01"]
    );
    assert_eq!(
        titles(TransactionFilter {
            scope: Scope::User(member.user_id),
            ..Default::default()
        })
        .await,
        ["2026-11-05"]
    );
    assert!(
        titles(TransactionFilter {
            scope: Scope::Mine,
            ..Default::default()
        })
        .await
        .is_empty()
    );

    let page = test
        .app
        .transactions
        .list(
            &setup.ctx,
            &TransactionFilter::default(),
            PageRequest::new(Some(2), Some(2)).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!((page.total, page.items.len()), (3, 1));
    let bad = TransactionFilter {
        kind: Some("refund".into()),
        ..Default::default()
    };
    assert!(
        test.app
            .transactions
            .list(&setup.ctx, &bad, PageRequest::default())
            .await
            .is_err()
    );
}
