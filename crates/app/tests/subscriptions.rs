#![allow(clippy::unwrap_used)] // test helpers outside #[test] functions

mod common;

use chrono::NaiveDate;
use common::*;
use saldo_app::accounts::AccountView;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::scope::Scope;
use saldo_app::subscriptions::{AcceptSubscription, SubscriptionCandidateView};
use saldo_core::{BillId, SplitId};

/// Pays `amount` from `from` to `to` on each date; returns the split IDs.
async fn pay(
    test: &TestApp,
    ctx: &HouseholdCtx,
    from: &AccountView,
    to: &AccountView,
    amount: &str,
    dates: &[(i32, u32, u32)],
) -> Vec<String> {
    let mut ids = Vec::new();
    for &(y, m, d) in dates {
        let mut s = split(1, from, to, amount);
        s.date = NaiveDate::from_ymd_opt(y, m, d).unwrap();
        let view = test
            .app
            .transactions
            .create(ctx, Origin::Api, group(vec![s]))
            .await
            .unwrap();
        ids.push(view.splits[0].id.clone());
    }
    ids
}

const MONTHLY: [(i32, u32, u32); 5] = [
    (2026, 7, 10),
    (2026, 8, 10),
    (2026, 9, 11),
    (2026, 10, 10),
    (2026, 11, 10),
];

struct Fixture {
    test: TestApp,
    ctx: HouseholdCtx,
    checking: AccountView,
    streaming: AccountView,
    splits: Vec<String>,
}

/// Today is 2026-11-13: five monthly 12.99 payments to "Streaming".
async fn fixture() -> Fixture {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let streaming = account(&test, &ctx, "Streaming", "expense", None).await;
    let splits = pay(&test, &ctx, &checking, &streaming, "12.99", &MONTHLY).await;
    Fixture {
        test,
        ctx,
        checking,
        streaming,
        splits,
    }
}

async fn candidates(f: &Fixture) -> Vec<SubscriptionCandidateView> {
    f.test
        .app
        .subscriptions
        .candidates(&f.ctx, Scope::Household)
        .await
        .unwrap()
}

fn accept(f: &Fixture, split_ids: &[String]) -> AcceptSubscription {
    AcceptSubscription {
        account_id: account_id(&f.streaming),
        currency: "EUR".into(),
        name: "Streaming".into(),
        frequency: "monthly".into(),
        amount_min: "12.99".into(),
        amount_max: "12.99".into(),
        split_ids: split_ids.iter().map(|id| id.parse().unwrap()).collect(),
    }
}

#[tokio::test]
async fn suggests_and_accepts_a_monthly_payment_as_a_bill() {
    let f = fixture().await;
    let found = candidates(&f).await;
    assert_eq!(found.len(), 1);
    let c = &found[0];
    assert_eq!(
        (
            c.account_name.as_str(),
            c.frequency.as_str(),
            c.amount_min.as_str(),
            c.next_due.as_str()
        ),
        ("Streaming", "monthly", "12.99", "2026-12-10")
    );
    assert_eq!(c.split_ids, f.splits);

    let bill = f
        .test
        .app
        .subscriptions
        .accept(&f.ctx, Origin::Api, accept(&f, &c.split_ids))
        .await
        .unwrap();
    assert_eq!(bill.name, "Streaming");
    let bill_id: BillId = bill.id.parse().unwrap();
    let periods = f.test.app.bills.periods(&f.ctx, bill_id).await.unwrap();
    let summary: Vec<_> = periods
        .iter()
        .rev()
        .map(|p| (p.due_date.as_str(), p.status.as_str()))
        .collect();
    // Every past payment pays its period; the bill does not start overdue.
    assert_eq!(
        summary,
        vec![
            ("2026-07-10", "paid"),
            ("2026-08-10", "paid"),
            ("2026-09-10", "paid"),
            ("2026-10-10", "paid"),
            ("2026-11-10", "paid"),
            ("2026-12-10", "open"),
            ("2027-01-10", "open"),
        ]
    );
    // The payments carry the bill now, so the suggestion is gone.
    assert!(candidates(&f).await.is_empty());
    let types: Vec<String> = events(&f.test, &f.ctx)
        .await
        .into_iter()
        .map(|e| e.event_type)
        .collect();
    assert!(types.contains(&"bill.created".to_string()));
    assert!(types.contains(&"bill.paid".to_string()));
}

#[tokio::test]
async fn a_gap_in_the_payments_is_skipped_not_left_overdue() {
    let f = fixture().await;
    // Accept without the September payment: its period must not stay open.
    let mut ids = f.splits.clone();
    ids.remove(2);
    let bill = f
        .test
        .app
        .subscriptions
        .accept(&f.ctx, Origin::Api, accept(&f, &ids))
        .await
        .unwrap();
    let periods = f
        .test
        .app
        .bills
        .periods(&f.ctx, bill.id.parse().unwrap())
        .await
        .unwrap();
    let september = periods.iter().find(|p| p.due_date == "2026-09-10").unwrap();
    assert_eq!(september.status, "skipped");
}

#[tokio::test]
async fn accepting_checks_the_payments() {
    let f = fixture().await;
    let other = account(&f.test, &f.ctx, "Gym", "expense", None).await;
    let gym = pay(
        &f.test,
        &f.ctx,
        &f.checking,
        &other,
        "30.00",
        &[(2026, 11, 1)],
    )
    .await;
    let unknown = SplitId(uuid::Uuid::from_u128(7)).to_string();
    for ids in [vec![], vec![unknown], gym] {
        let err = f
            .test
            .app
            .subscriptions
            .accept(&f.ctx, Origin::Api, accept(&f, &ids))
            .await
            .unwrap_err();
        assert!(
            matches!(err, AppError::Invalid { ref field, .. } if field == "split_ids"),
            "{ids:?}: {err:?}"
        );
    }
    // Nothing was created by the failed attempts.
    assert_eq!(count(&f.test, "bills").await, 0);
}

#[tokio::test]
async fn dismissed_suggestions_stay_away_until_restored() {
    let f = fixture().await;
    let service = &f.test.app.subscriptions;
    let streaming = account_id(&f.streaming);
    service
        .dismiss(&f.ctx, Origin::Api, streaming, "EUR")
        .await
        .unwrap();
    assert!(candidates(&f).await.is_empty());
    assert_eq!(
        last_event(&f.test, &f.ctx).await.event_type,
        "subscription_dismissal.created"
    );
    // Twice is a no-op without a second event.
    let before = count(&f.test, "events").await;
    service
        .dismiss(&f.ctx, Origin::Api, streaming, "EUR")
        .await
        .unwrap();
    assert_eq!(count(&f.test, "events").await, before);

    service
        .restore(&f.ctx, Origin::Api, streaming, "EUR")
        .await
        .unwrap();
    assert_eq!(candidates(&f).await.len(), 1);
    assert_eq!(
        last_event(&f.test, &f.ctx).await.event_type,
        "subscription_dismissal.deleted"
    );
    let again = service.restore(&f.ctx, Origin::Api, streaming, "EUR").await;
    assert!(matches!(again, Err(AppError::NotFound)));

    let asset = service
        .dismiss(&f.ctx, Origin::Api, account_id(&f.checking), "EUR")
        .await;
    assert!(matches!(asset, Err(AppError::Invalid { ref field, .. }) if field == "account_id"));
}
