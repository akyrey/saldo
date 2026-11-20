//! Alerts (Phase 10): budget thresholds, the forecast going below zero, the
//! weekly digest and large transactions. The test clock starts on Friday
//! 2026-11-13 at 09:00 UTC.

#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use chrono::TimeDelta;
use common::*;
use saldo_app::accounts::AccountView;
use saldo_app::budgets::{NewBudget, NewBudgetLimit};
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::identity::HouseholdUpdate;
use saldo_app::transactions::TransactionInput;
use saldo_core::GroupId;

async fn of_type(test: &TestApp, ctx: &HouseholdCtx, event_type: &str) -> Vec<serde_json::Value> {
    events(test, ctx)
        .await
        .into_iter()
        .filter(|e| e.event_type == event_type)
        .map(|e| e.payload["data"].clone())
        .collect()
}

/// A budget with a 100.00 limit for November.
async fn groceries_budget(test: &TestApp, ctx: &HouseholdCtx) -> String {
    let budget = test
        .app
        .budgets
        .create(
            ctx,
            Origin::Api,
            NewBudget {
                name: "Groceries".into(),
                ..NewBudget::default()
            },
        )
        .await
        .unwrap();
    let limit = NewBudgetLimit {
        start_date: date(1),
        end_date: date(30),
        amount: "100".into(),
        currency: None,
        notes: None,
    };
    test.app
        .budgets
        .create_limit(ctx, Origin::Api, budget.id.parse().unwrap(), limit)
        .await
        .unwrap();
    budget.id
}

fn spend(from: &AccountView, to: &AccountView, amount: &str, budget: &str) -> TransactionInput {
    let mut input = split(10, from, to, amount);
    input.budget_id = Some(budget.parse().unwrap());
    group(vec![input])
}

#[tokio::test]
async fn budget_thresholds_alert_once_per_crossing() {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let shop = account(&test, &ctx, "Shop", "expense", None).await;
    let budget = groceries_budget(&test, &ctx).await;
    let alerts = &test.app.alerts;

    let first = test
        .app
        .transactions
        .create(&ctx, Origin::Api, spend(&checking, &shop, "85", &budget))
        .await
        .unwrap();
    alerts.run(&ctx).await.unwrap();
    alerts.run(&ctx).await.unwrap();
    let reached = of_type(&test, &ctx, "budget.threshold_reached").await;
    assert_eq!(
        reached.len(),
        1,
        "80% once, however often the scheduler runs"
    );
    assert_eq!(reached[0]["threshold"], 80);
    assert_eq!(reached[0]["total"], "100.00");
    assert_eq!(reached[0]["budget"]["name"], "Groceries");

    test.app
        .transactions
        .create(&ctx, Origin::Api, spend(&checking, &shop, "20", &budget))
        .await
        .unwrap();
    alerts.run(&ctx).await.unwrap();
    assert_eq!(
        of_type(&test, &ctx, "budget.threshold_reached").await.len(),
        2
    );

    // A refund (the first purchase shrinks) drops below 80%, then climbs again.
    let id: GroupId = first.id.parse().unwrap();
    let mut smaller = saldo_app::transactions::input_from_view(&first).unwrap();
    smaller.splits[0].amount = "10".into();
    test.app
        .transactions
        .replace(&ctx, Origin::Api, id, smaller)
        .await
        .unwrap();
    alerts.run(&ctx).await.unwrap();
    assert_eq!(
        of_type(&test, &ctx, "budget.threshold_reached").await.len(),
        2
    );
    test.app
        .transactions
        .create(&ctx, Origin::Api, spend(&checking, &shop, "55", &budget))
        .await
        .unwrap();
    alerts.run(&ctx).await.unwrap();
    let reached = of_type(&test, &ctx, "budget.threshold_reached").await;
    assert_eq!(reached.len(), 3, "80% again after dropping below it");
    assert!(reached.iter().all(|r| r["alert_key"].is_string()));
}

#[tokio::test]
async fn forecast_below_zero_alerts_while_it_lasts() {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let salary = account(&test, &ctx, "Salary", "revenue", None).await;
    let rent = account(&test, &ctx, "Rent", "expense", None).await;
    test.app
        .transactions
        .create(
            &ctx,
            Origin::Api,
            group(vec![split(1, &salary, &checking, "100")]),
        )
        .await
        .unwrap();
    // Dated after today: the forecast counts it.
    let future = test
        .app
        .transactions
        .create(
            &ctx,
            Origin::Api,
            group(vec![split(25, &checking, &rent, "300")]),
        )
        .await
        .unwrap();
    test.app.alerts.run(&ctx).await.unwrap();
    test.app.alerts.run(&ctx).await.unwrap();
    let alerts = of_type(&test, &ctx, "forecast.below_zero").await;
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0]["account"]["name"], "Checking");
    assert_eq!(alerts[0]["lowest"]["balance"], "-200.00");

    test.app
        .transactions
        .delete(&ctx, Origin::Api, future.id.parse().unwrap())
        .await
        .unwrap();
    test.app.alerts.run(&ctx).await.unwrap();
    let held: (i64,) =
        sqlx::query_as("SELECT count(*) FROM alert_states WHERE key LIKE 'forecast:%'")
            .fetch_one(&test.app.pool)
            .await
            .unwrap();
    assert_eq!(held.0, 0, "forgotten once the forecast recovers");
}

#[tokio::test]
async fn the_digest_goes_out_once_a_week() {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let shop = account(&test, &ctx, "Shop", "expense", None).await;
    test.app
        .transactions
        .create(
            &ctx,
            Origin::Api,
            group(vec![split(4, &checking, &shop, "12.50")]),
        )
        .await
        .unwrap();

    // Friday 13th: this week's (W46) digest is due, covering 2 to 8 November.
    test.app.alerts.run(&ctx).await.unwrap();
    test.app.alerts.run(&ctx).await.unwrap();
    let digests = of_type(&test, &ctx, "digest.weekly").await;
    assert_eq!(digests.len(), 1);
    assert_eq!(digests[0]["week"]["from"], "2026-11-02");
    assert_eq!(digests[0]["currencies"][0]["expenses"], "12.50");

    // Monday 16th, 07:00 UTC: not yet; 08:00: next week's.
    test.clock.advance(TimeDelta::hours(70));
    test.app.alerts.run(&ctx).await.unwrap();
    assert_eq!(of_type(&test, &ctx, "digest.weekly").await.len(), 1);
    test.clock.advance(TimeDelta::hours(1));
    test.app.alerts.run(&ctx).await.unwrap();
    assert_eq!(of_type(&test, &ctx, "digest.weekly").await.len(), 2);
    let keys: Vec<(String,)> =
        sqlx::query_as("SELECT key FROM alert_states WHERE key LIKE 'digest:%'")
            .fetch_all(&test.app.pool)
            .await
            .unwrap();
    assert_eq!(keys, [("digest:2026-W47".to_string(),)]);
}

#[tokio::test]
async fn large_transactions_alert_for_people_and_rules_only() {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let shop = account(&test, &ctx, "Shop", "expense", None).await;
    let tx = &test.app.transactions;

    // Off by default.
    tx.create(
        &ctx,
        Origin::Api,
        group(vec![split(1, &checking, &shop, "900")]),
    )
    .await
    .unwrap();
    assert!(of_type(&test, &ctx, "transaction.large").await.is_empty());

    let update = HouseholdUpdate {
        large_transaction_amount: Some(Some("500".into())),
        ..Default::default()
    };
    test.app
        .identity
        .update_household(&ctx, Origin::Api, update)
        .await
        .unwrap();
    let big = tx
        .create(
            &ctx,
            Origin::Api,
            group(vec![
                split(2, &checking, &shop, "500"),
                split(2, &checking, &shop, "20"),
            ]),
        )
        .await
        .unwrap();
    tx.create(
        &ctx,
        Origin::Import,
        group(vec![split(3, &checking, &shop, "800")]),
    )
    .await
    .unwrap();
    let large = of_type(&test, &ctx, "transaction.large").await;
    assert_eq!(large.len(), 1, "imports do not alert");
    assert_eq!(large[0]["split_id"], big.splits[0].id.as_str());
    assert_eq!(large[0]["threshold"], "500.00");

    // Editing the large split's notes alerts nothing; raising the small one does.
    let mut edit = saldo_app::transactions::input_from_view(&big).unwrap();
    edit.splits[0].notes = Some("checked".into());
    edit.splits[1].amount = "650".into();
    tx.replace(&ctx, Origin::Api, big.id.parse().unwrap(), edit)
        .await
        .unwrap();
    let large = of_type(&test, &ctx, "transaction.large").await;
    let mut ids: Vec<&str> = large
        .iter()
        .map(|l| l["split_id"].as_str().unwrap())
        .collect();
    ids.sort_unstable();
    let mut expected = [big.splits[0].id.as_str(), big.splits[1].id.as_str()];
    expected.sort_unstable();
    assert_eq!(ids, expected);
}
