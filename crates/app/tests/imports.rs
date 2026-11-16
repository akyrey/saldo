//! Building blocks for importers: external refs, import runs, currencies,
//! household lookup, and the import exemption for archived accounts.

mod common;

use common::*;
use saldo_app::accounts::AccountUpdate;
use saldo_app::ctx::Origin;
use saldo_app::currencies::{self, CurrencyView, Ensured};
use saldo_app::error::AppError;
use saldo_app::external_refs::{ExternalRefs, FIREFLY};
use saldo_app::import_runs;
use serde_json::json;

#[tokio::test]
async fn external_refs_round_trip_and_skip_unchanged_writes() {
    let (test, setup) = with_household().await;
    let mut conn = test.app.pool.acquire().await.unwrap();
    let mut refs = ExternalRefs::load(&mut conn, &setup.ctx, FIREFLY)
        .await
        .unwrap();
    assert_eq!(refs.resolve("account", "7"), None);

    refs.upsert(&mut conn, "account", "7", "local-a", "t1")
        .await
        .unwrap();
    refs.upsert(&mut conn, "account", "8", "local-b", "t1")
        .await
        .unwrap();
    // Same mapping again: no write, so updated_at keeps its first value.
    refs.upsert(&mut conn, "account", "7", "local-a", "t2")
        .await
        .unwrap();
    let (stamp,): (String,) =
        sqlx::query_as("SELECT updated_at FROM external_refs WHERE external_id = '7'")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    assert_eq!(stamp, "t1");
    refs.upsert(&mut conn, "account", "7", "local-c", "t3")
        .await
        .unwrap();

    let reloaded = ExternalRefs::load(&mut conn, &setup.ctx, FIREFLY)
        .await
        .unwrap();
    assert_eq!(reloaded.resolve("account", "7"), Some("local-c"));
    let mut entries = reloaded.entries("account");
    entries.sort();
    assert_eq!(
        entries,
        [
            ("7".to_string(), "local-c".to_string()),
            ("8".to_string(), "local-b".to_string())
        ]
    );

    refs.remove(&mut conn, "account", "8").await.unwrap();
    assert_eq!(refs.resolve("account", "8"), None);
    drop(conn); // the in-memory pool has a single connection
    assert_eq!(count(&test, "external_refs").await, 1);
    // Other households see nothing.
    let other = other_household(&test).await;
    let mut conn = test.app.pool.acquire().await.unwrap();
    assert!(
        ExternalRefs::load(&mut conn, &other, FIREFLY)
            .await
            .unwrap()
            .entries("account")
            .is_empty()
    );
}

#[tokio::test]
async fn import_runs_record_status_and_summary() {
    let (test, setup) = with_household().await;
    let id = import_runs::start(&test.app.pool, &*test.clock, &setup.ctx, FIREFLY)
        .await
        .unwrap();
    import_runs::finish(
        &test.app.pool,
        &*test.clock,
        &id,
        true,
        &json!({ "accounts": { "created": 3 } }),
    )
    .await
    .unwrap();
    let (status, summary): (String, String) =
        sqlx::query_as("SELECT status, summary FROM import_runs WHERE id = ?")
            .bind(&id)
            .fetch_one(&test.app.pool)
            .await
            .unwrap();
    assert_eq!(status, "succeeded");
    assert!(summary.contains("\"created\":3"));
}

#[tokio::test]
async fn currencies_are_added_once_and_conflicts_reported() {
    let test = test_app().await;
    let mut conn = test.app.pool.acquire().await.unwrap();
    let ticket = CurrencyView {
        code: "TKT".into(),
        name: "Meal ticket".into(),
        symbol: "T".into(),
        decimal_places: 0,
    };
    assert_eq!(
        currencies::ensure(&mut conn, &ticket).await.unwrap(),
        Ensured::Created
    );
    assert_eq!(
        currencies::ensure(&mut conn, &ticket).await.unwrap(),
        Ensured::Unchanged
    );
    let changed = CurrencyView {
        decimal_places: 2,
        ..ticket.clone()
    };
    assert!(matches!(
        currencies::ensure(&mut conn, &changed).await,
        Err(AppError::Conflict(_))
    ));
    let bad = CurrencyView {
        code: "ticket".into(),
        ..ticket
    };
    assert!(matches!(
        currencies::ensure(&mut conn, &bad).await,
        Err(AppError::Invalid { .. })
    ));
}

#[tokio::test]
async fn households_are_found_by_id_or_name() {
    let (test, setup) = with_household().await;
    let by_name = test.app.identity.household_context("Home").await.unwrap();
    assert_eq!(by_name, setup.ctx);
    let by_id = test
        .app
        .identity
        .household_context(&setup.household.id)
        .await
        .unwrap();
    assert_eq!(by_id, setup.ctx);
    assert!(matches!(
        test.app.identity.household_context("Nope").await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn imports_may_book_on_archived_accounts_but_users_may_not() {
    let (test, setup) = with_household().await;
    let old = account(&test, &setup.ctx, "Old account", "asset", None).await;
    let food = account(&test, &setup.ctx, "Food", "expense", None).await;
    let archive = AccountUpdate {
        active: Some(false),
        ..AccountUpdate::default()
    };
    test.app
        .accounts
        .update(&setup.ctx, Origin::Api, account_id(&old), archive)
        .await
        .unwrap();

    let by_user = test
        .app
        .transactions
        .create(
            &setup.ctx,
            Origin::Api,
            group(vec![split(1, &old, &food, "5")]),
        )
        .await;
    assert!(matches!(by_user, Err(AppError::Validation(_))));
    let by_import = test
        .app
        .transactions
        .create(
            &setup.ctx,
            Origin::Import,
            group(vec![split(1, &old, &food, "5")]),
        )
        .await
        .unwrap();
    assert_eq!(last_event(&test, &setup.ctx).await.origin, "import");

    // The `_in` variants see uncommitted rows of the same transaction.
    let mut tx = saldo_app::db::begin(&test.app.pool).await.unwrap();
    let view = test
        .app
        .transactions
        .get_in(&mut tx, &setup.ctx, by_import.id.parse().unwrap())
        .await
        .unwrap();
    assert_eq!(view, by_import);
    let balance = test
        .app
        .accounts
        .balance_in(&mut tx, &setup.ctx, account_id(&old), None)
        .await
        .unwrap();
    assert_eq!(balance.balance, "-5.00");
    tx.rollback().await.unwrap();
}
