mod common;

use common::*;
use saldo_app::accounts::{AccountFilter, AccountUpdate, AccountView, NewAccount};
use saldo_app::ctx::Origin;
use saldo_app::error::AppError;
use saldo_app::page::{Page, PageRequest};
use saldo_app::scope::Scope;

fn new_account(name: &str, account_type: &str) -> NewAccount {
    NewAccount {
        name: name.to_string(),
        account_type: account_type.to_string(),
        ..NewAccount::default()
    }
}

#[tokio::test]
async fn creates_an_account_with_defaults_and_an_event() {
    let (test, setup) = with_household().await;
    let view = account(&test, &setup.ctx, " Checking ", "asset", None).await;

    assert_eq!(view.name, "Checking");
    assert_eq!(view.account_type, "asset");
    assert_eq!(view.currency, "EUR");
    assert_eq!(view.virtual_balance, "0.00");
    assert!(view.active && view.include_net_worth);

    let event = last_event(&test, &setup.ctx).await;
    assert_eq!(event.event_type, "account.created");
    assert_eq!(event.entity_id, view.id);
    assert_eq!(event.payload["data"], serde_json::to_value(&view).unwrap());
}

#[tokio::test]
async fn stores_currency_virtual_balance_and_details() {
    let (test, setup) = with_household().await;
    let input = NewAccount {
        currency: Some("JPY".to_string()),
        virtual_balance: Some("1500".to_string()),
        asset_role: Some("savings".to_string()),
        iban: Some("IT60X0542811101000000123456".to_string()),
        owner_user_id: Some(setup.ctx.user_id),
        ..new_account("Yen", "asset")
    };
    let view = test
        .app
        .accounts
        .create(&setup.ctx, Origin::Api, input)
        .await
        .unwrap();
    assert_eq!(
        (view.currency.as_str(), view.virtual_balance.as_str()),
        ("JPY", "1500")
    );
    assert_eq!(view.asset_role.as_deref(), Some("savings"));
    assert_eq!(view.owner_user_id, Some(setup.user.id.clone()));
    assert_eq!(
        test.app
            .accounts
            .get(&setup.ctx, account_id(&view))
            .await
            .unwrap(),
        view
    );
}

#[tokio::test]
async fn rejects_invalid_accounts_without_writing() {
    let (test, setup) = with_household().await;
    let before = count(&test, "events").await;
    let stranger = saldo_core::UserId(uuid::Uuid::from_u128(77));
    let cases = [
        ("type", new_account("A", "equity")),
        ("type", new_account("A", "savings")),
        ("name", new_account("  ", "asset")),
        (
            "currency",
            NewAccount {
                currency: Some("XXX".into()),
                ..new_account("A", "asset")
            },
        ),
        (
            "asset_role",
            NewAccount {
                asset_role: Some("savings".into()),
                ..new_account("A", "expense")
            },
        ),
        (
            "asset_role",
            NewAccount {
                asset_role: Some("piggy".into()),
                ..new_account("A", "asset")
            },
        ),
        (
            "liability_type",
            NewAccount {
                liability_type: Some("loan".into()),
                ..new_account("A", "asset")
            },
        ),
        (
            "interest_rate",
            NewAccount {
                interest_rate: Some("abc".into()),
                ..new_account("A", "liability")
            },
        ),
        (
            "virtual_balance",
            NewAccount {
                virtual_balance: Some("1.234".into()),
                ..new_account("A", "asset")
            },
        ),
        (
            "owner_user_id",
            NewAccount {
                owner_user_id: Some(stranger),
                ..new_account("A", "asset")
            },
        ),
    ];
    for (expected, input) in cases {
        let err = test
            .app
            .accounts
            .create(&setup.ctx, Origin::Api, input)
            .await
            .unwrap_err();
        assert!(
            matches!(&err, AppError::Invalid { field, .. } if field == expected),
            "{expected}: {err:?}"
        );
    }
    assert_eq!(count(&test, "events").await, before);
}

#[tokio::test]
async fn liability_details_are_accepted_on_liabilities() {
    let (test, setup) = with_household().await;
    let input = NewAccount {
        liability_type: Some("mortgage".into()),
        liability_direction: Some("i_owe".into()),
        interest_rate: Some("3.25".into()),
        interest_period: Some("monthly".into()),
        ..new_account("House", "liability")
    };
    let view = test
        .app
        .accounts
        .create(&setup.ctx, Origin::Api, input)
        .await
        .unwrap();
    assert_eq!(view.interest_rate.as_deref(), Some("3.25"));
}

#[tokio::test]
async fn names_are_unique_per_type() {
    let (test, setup) = with_household().await;
    account(&test, &setup.ctx, "Cash", "asset", None).await;
    let duplicate = test
        .app
        .accounts
        .create(&setup.ctx, Origin::Api, new_account("Cash", "asset"))
        .await;
    assert!(matches!(duplicate, Err(AppError::Conflict(_))));
    assert!(
        test.app
            .accounts
            .create(&setup.ctx, Origin::Api, new_account("Cash", "expense"))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn updates_fields_but_never_the_type() {
    let (test, setup) = with_household().await;
    let created = test
        .app
        .accounts
        .create(
            &setup.ctx,
            Origin::Api,
            NewAccount {
                notes: Some("old".into()),
                ..new_account("Checking", "asset")
            },
        )
        .await
        .unwrap();
    let id = account_id(&created);

    let update = AccountUpdate {
        name: Some("Main".into()),
        account_type: Some("asset".into()),
        notes: Some(None),
        active: Some(false),
        virtual_balance: Some("-50".into()),
        ..AccountUpdate::default()
    };
    let view = test
        .app
        .accounts
        .update(&setup.ctx, Origin::Api, id, update)
        .await
        .unwrap();
    assert_eq!(
        (view.name.as_str(), view.notes.clone(), view.active),
        ("Main", None, false)
    );
    assert_eq!(view.virtual_balance, "-50.00");
    let event = last_event(&test, &setup.ctx).await;
    assert_eq!(event.event_type, "account.updated");
    assert_eq!(event.payload["data"]["name"], "Main");

    let change_type = AccountUpdate {
        account_type: Some("expense".into()),
        ..AccountUpdate::default()
    };
    let err = test
        .app
        .accounts
        .update(&setup.ctx, Origin::Api, id, change_type)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Invalid { ref field, .. } if field == "type"));
    let blank = AccountUpdate {
        name: Some(" ".into()),
        ..AccountUpdate::default()
    };
    assert!(
        test.app
            .accounts
            .update(&setup.ctx, Origin::Api, id, blank)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn deletes_only_accounts_without_transactions() {
    let (test, setup) = with_household().await;
    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let food = account(&test, &setup.ctx, "Food", "expense", None).await;
    let spare = account(&test, &setup.ctx, "Spare", "asset", None).await;
    test.app
        .transactions
        .create(
            &setup.ctx,
            Origin::Api,
            group(vec![split(1, &checking, &food, "5")]),
        )
        .await
        .unwrap();

    let used = test
        .app
        .accounts
        .delete(&setup.ctx, Origin::Api, account_id(&checking))
        .await;
    assert!(matches!(used, Err(AppError::Conflict(_))));

    test.app
        .accounts
        .delete(&setup.ctx, Origin::Api, account_id(&spare))
        .await
        .unwrap();
    let event = last_event(&test, &setup.ctx).await;
    assert_eq!(
        (event.event_type.as_str(), event.entity_id.as_str()),
        ("account.deleted", spare.id.as_str())
    );
    assert!(matches!(
        test.app.accounts.get(&setup.ctx, account_id(&spare)).await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn equity_accounts_cannot_be_deleted() {
    let (test, setup) = with_household().await;
    let filter = AccountFilter {
        account_type: Some("equity".into()),
        ..AccountFilter::default()
    };
    let equity = test
        .app
        .accounts
        .list(&setup.ctx, &filter, PageRequest::default())
        .await
        .unwrap();
    assert_eq!(equity.total, 2);
    let result = test
        .app
        .accounts
        .delete(&setup.ctx, Origin::Api, account_id(&equity.items[0]))
        .await;
    assert!(matches!(result, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn searches_accounts_by_name_ignoring_case() {
    let (test, setup) = with_household().await;
    account(&test, &setup.ctx, "Supermarket Rossi", "expense", None).await;
    account(&test, &setup.ctx, "Rossi Salary", "revenue", None).await;
    account(&test, &setup.ctx, "Bakery", "expense", None).await;

    let search = |q: &str, account_type: Option<&str>| {
        let app = test.app.clone();
        let ctx = setup.ctx;
        let filter = AccountFilter {
            account_type: account_type.map(str::to_string),
            q: Some(q.to_string()),
            ..Default::default()
        };
        async move {
            app.accounts
                .list(&ctx, &filter, PageRequest::default())
                .await
                .unwrap()
        }
    };
    let names = |page: Page<AccountView>| -> Vec<String> {
        page.items.into_iter().map(|a| a.name).collect()
    };

    assert_eq!(
        names(search("rossi", None).await),
        ["Supermarket Rossi", "Rossi Salary"]
    );
    assert_eq!(
        names(search("ROSSI", Some("revenue")).await),
        ["Rossi Salary"]
    );
    assert_eq!(search("nothing like it", None).await.total, 0);
    assert_eq!(
        search("  ", Some("expense")).await.total,
        2,
        "a blank search matches all"
    );
}

#[tokio::test]
async fn lists_with_filters_scope_and_pages() {
    let (test, setup) = with_household().await;
    let member = add_member(&test, &setup.ctx, "partner@example.com").await;
    account(&test, &setup.ctx, "Joint", "asset", None).await;
    let mine = NewAccount {
        owner_user_id: Some(setup.ctx.user_id),
        ..new_account("Mine", "asset")
    };
    test.app
        .accounts
        .create(&setup.ctx, Origin::Api, mine)
        .await
        .unwrap();
    let theirs = NewAccount {
        owner_user_id: Some(member.user_id),
        ..new_account("Theirs", "asset")
    };
    test.app
        .accounts
        .create(&member, Origin::Api, theirs)
        .await
        .unwrap();

    let list = |filter: AccountFilter| {
        let app = test.app.clone();
        let ctx = setup.ctx;
        async move {
            app.accounts
                .list(&ctx, &filter, PageRequest::default())
                .await
                .unwrap()
        }
    };
    let assets = list(AccountFilter {
        account_type: Some("asset".into()),
        ..Default::default()
    })
    .await;
    assert_eq!(assets.total, 3);
    let names: Vec<_> = list(AccountFilter {
        scope: Scope::Mine,
        ..Default::default()
    })
    .await
    .items
    .into_iter()
    .map(|a| a.name)
    .collect();
    assert_eq!(names, ["Mine"]);
    let theirs = list(AccountFilter {
        scope: Scope::User(member.user_id),
        ..Default::default()
    })
    .await;
    assert_eq!(theirs.items[0].name, "Theirs");
    assert_eq!(
        list(AccountFilter {
            active: Some(false),
            ..Default::default()
        })
        .await
        .total,
        0
    );

    let page = test
        .app
        .accounts
        .list(
            &setup.ctx,
            &AccountFilter::default(),
            PageRequest::new(Some(2), Some(2)).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!((page.total, page.items.len()), (5, 2));
    let bad = AccountFilter {
        account_type: Some("nope".into()),
        ..Default::default()
    };
    assert!(
        test.app
            .accounts
            .list(&setup.ctx, &bad, PageRequest::default())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn balance_sums_up_to_a_date() {
    let (test, setup) = with_household().await;
    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let salary = account(&test, &setup.ctx, "Salary", "revenue", None).await;
    let food = account(&test, &setup.ctx, "Food", "expense", None).await;
    let input = group(vec![
        split(1, &salary, &checking, "2000"),
        split(5, &checking, &food, "42.50"),
    ]);
    test.app
        .transactions
        .create(&setup.ctx, Origin::Api, input)
        .await
        .unwrap();

    let all = test
        .app
        .accounts
        .balance(&setup.ctx, account_id(&checking), None)
        .await
        .unwrap();
    assert_eq!(
        (
            all.balance.as_str(),
            all.currency.as_str(),
            all.date.clone()
        ),
        ("1957.50", "EUR", None)
    );
    let early = test
        .app
        .accounts
        .balance(&setup.ctx, account_id(&checking), Some(date(3)))
        .await
        .unwrap();
    assert_eq!(
        (early.balance.as_str(), early.date.as_deref()),
        ("2000.00", Some("2026-11-03"))
    );
    let spent = test
        .app
        .accounts
        .balance(&setup.ctx, account_id(&food), None)
        .await
        .unwrap();
    assert_eq!(spent.balance, "42.50");
}
