//! A context for household B can neither read nor change household A.

mod common;

use common::*;
use saldo_app::accounts::{AccountFilter, AccountUpdate};
use saldo_app::categories::{CategoryUpdate, NewCategory};
use saldo_app::ctx::Origin;
use saldo_app::error::AppError;
use saldo_app::page::PageRequest;
use saldo_app::tags::{NewTag, TagUpdate};
use saldo_app::transactions::TransactionFilter;
use saldo_core::GroupId;

fn not_found<T: std::fmt::Debug>(result: Result<T, AppError>) -> bool {
    matches!(result, Err(AppError::NotFound))
}

#[tokio::test]
async fn another_household_cannot_read_or_write() {
    let (test, setup) = with_household().await;
    let a = setup.ctx;
    let b = other_household(&test).await;
    let checking = account(&test, &a, "Checking", "asset", None).await;
    let food = account(&test, &a, "Food", "expense", None).await;
    let category = test
        .app
        .categories
        .create(
            &a,
            Origin::Api,
            NewCategory {
                name: "Food".into(),
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
            &a,
            Origin::Api,
            NewTag {
                name: "t".into(),
                description: None,
            },
        )
        .await
        .unwrap();
    let group_view = test
        .app
        .transactions
        .create(
            &a,
            Origin::Api,
            group(vec![split(1, &checking, &food, "5")]),
        )
        .await
        .unwrap();
    let group_id: GroupId = group_view.id.parse().unwrap();
    let account_events = count(&test, "events").await;

    // Reads
    assert!(not_found(
        test.app.accounts.get(&b, account_id(&checking)).await
    ));
    assert!(not_found(
        test.app
            .accounts
            .balance(&b, account_id(&checking), None)
            .await
    ));
    assert!(not_found(
        test.app
            .categories
            .get(&b, category.id.parse().unwrap())
            .await
    ));
    assert!(not_found(
        test.app.tags.get(&b, tag.id.parse().unwrap()).await
    ));
    assert!(not_found(test.app.transactions.get(&b, group_id).await));
    assert_eq!(
        test.app
            .accounts
            .list(&b, &AccountFilter::default(), PageRequest::default())
            .await
            .unwrap()
            .total,
        0
    );
    assert_eq!(
        test.app
            .transactions
            .list(&b, &TransactionFilter::default(), PageRequest::default())
            .await
            .unwrap()
            .total,
        0
    );
    assert_eq!(
        test.app
            .categories
            .list(&b, PageRequest::default())
            .await
            .unwrap()
            .total,
        0
    );
    assert!(events(&test, &b).await.is_empty());

    // Writes
    assert!(not_found(
        test.app
            .accounts
            .update(
                &b,
                Origin::Api,
                account_id(&checking),
                AccountUpdate::default()
            )
            .await
    ));
    assert!(not_found(
        test.app
            .accounts
            .delete(&b, Origin::Api, account_id(&food))
            .await
    ));
    assert!(not_found(
        test.app
            .categories
            .update(
                &b,
                Origin::Api,
                category.id.parse().unwrap(),
                CategoryUpdate::default()
            )
            .await
    ));
    assert!(not_found(
        test.app
            .categories
            .delete(&b, Origin::Api, category.id.parse().unwrap())
            .await
    ));
    assert!(not_found(
        test.app
            .tags
            .update(
                &b,
                Origin::Api,
                tag.id.parse().unwrap(),
                TagUpdate::default()
            )
            .await
    ));
    assert!(not_found(
        test.app
            .tags
            .delete(&b, Origin::Api, tag.id.parse().unwrap())
            .await
    ));
    assert!(not_found(
        test.app
            .transactions
            .delete(&b, Origin::Api, group_id)
            .await
    ));
    let replace = test
        .app
        .transactions
        .replace(
            &b,
            Origin::Api,
            group_id,
            group(vec![split(1, &checking, &food, "1")]),
        )
        .await;
    assert!(not_found(replace));

    // B cannot book on A's accounts, nor use A's category.
    let mut input = split(1, &checking, &food, "5");
    input.category_id = Some(category.id.parse().unwrap());
    match test
        .app
        .transactions
        .create(&b, Origin::Api, group(vec![input]))
        .await
    {
        Err(AppError::Validation(errors)) => {
            let codes: Vec<_> = errors.iter().map(|e| e.code()).collect();
            assert_eq!(
                codes,
                ["unknown_reference", "unknown_account", "unknown_account"]
            );
        }
        other => panic!("expected validation errors, got {other:?}"),
    }
    // B cannot make A's member the owner of its account.
    let mut owned = saldo_app::accounts::NewAccount {
        name: "X".into(),
        account_type: "asset".into(),
        ..Default::default()
    };
    owned.owner_user_id = Some(a.user_id);
    assert!(matches!(
        test.app.accounts.create(&b, Origin::Api, owned).await,
        Err(AppError::Invalid { .. })
    ));

    assert_eq!(count(&test, "events").await, account_events);
    assert_eq!(
        test.app.transactions.get(&a, group_id).await.unwrap(),
        group_view
    );
}
