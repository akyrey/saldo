//! "Safe to spend this month" against a hand-computed household. Today is
//! Friday 2026-11-13: 18 days are left in November, today included.

#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use chrono::NaiveDate;
use common::*;
use saldo_app::accounts::NewAccount;
use saldo_app::bills::NewBill;
use saldo_app::budgets::{NewBudget, NewBudgetLimit};
use saldo_app::ctx::Origin;
use saldo_app::piggy_banks::NewPiggyBank;
use saldo_app::scope::Scope;

#[tokio::test]
async fn safe_to_spend_adds_income_and_subtracts_every_commitment() {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let app = &test.app;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let savings = app
        .accounts
        .create(
            &ctx,
            Origin::Api,
            NewAccount {
                name: "Savings".into(),
                account_type: "asset".into(),
                asset_role: Some("savings".into()),
                ..NewAccount::default()
            },
        )
        .await
        .unwrap();
    let salary = account(&test, &ctx, "Salary", "revenue", None).await;
    let shop = account(&test, &ctx, "Shop", "expense", None).await;
    let budget = app
        .budgets
        .create(
            &ctx,
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
        amount: "500".into(),
        currency: None,
        notes: None,
    };
    app.budgets
        .create_limit(&ctx, Origin::Api, budget.id.parse().unwrap(), limit)
        .await
        .unwrap();

    let mut shopping = split(5, &checking, &shop, "300");
    shopping.budget_id = Some(budget.id.parse().unwrap());
    for input in [
        split(1, &salary, &checking, "2000"),
        shopping,
        // Dated later this month: planned.
        split(20, &checking, &shop, "50"),
        split(25, &salary, &checking, "100"),
        split(28, &checking, &savings, "150"),
    ] {
        app.transactions
            .create(&ctx, Origin::Api, group(vec![input]))
            .await
            .unwrap();
    }
    app.bills
        .create(
            &ctx,
            Origin::Api,
            NewBill {
                name: "Internet".into(),
                amount_min: "60".into(),
                amount_max: "80".into(),
                currency: None,
                first_due_date: date(20),
                end_date: None,
                repeat_freq: "monthly".into(),
                skip: None,
                remind_days_before: None,
                remind_every_days: None,
                active: None,
                sort_order: None,
                notes: None,
            },
        )
        .await
        .unwrap();
    app.piggy_banks
        .create(
            &ctx,
            Origin::Api,
            NewPiggyBank {
                name: "Holiday".into(),
                currency: None,
                target_amount: Some("1200".into()),
                start_date: None,
                target_date: NaiveDate::from_ymd_opt(2027, 2, 28),
                active: None,
                sort_order: None,
                notes: None,
                account_ids: vec![account_id(&savings)],
            },
        )
        .await
        .unwrap();

    let view = app
        .safe_to_spend
        .current(&ctx, Scope::Household)
        .await
        .unwrap();
    let p = &view.parts;
    assert_eq!(
        (view.from.as_str(), view.to.as_str()),
        ("2026-11-01", "2026-11-30")
    );
    assert_eq!(p.income_received, "2000.00");
    assert_eq!(p.income_planned, "100.00");
    assert_eq!(p.spent, "300.00");
    assert_eq!(p.bills, "80.00", "at the highest amount");
    assert_eq!(p.planned, "50.00");
    assert_eq!(p.budgets, "200.00", "500 budgeted, 300 spent");
    // 1200 over November to February.
    assert_eq!(p.piggy_banks, "300.00");
    assert_eq!(p.savings_transfers, "150.00");
    // 2100 - (300 + 80 + 50 + 200 + 300 + 150) = 1020.
    assert_eq!(view.safe_to_spend, "1020.00");
    assert_eq!(view.days_left, 18);
    assert_eq!(view.per_day, "56.66");
    let parts: Vec<_> = view.items.iter().map(|i| i.part.as_str()).collect();
    for part in [
        "income_planned",
        "bills",
        "planned",
        "budgets",
        "piggy_banks",
        "savings_transfers",
    ] {
        assert!(parts.contains(&part), "{part} is listed");
    }
}

#[tokio::test]
async fn an_empty_household_has_nothing_to_spend() {
    let (test, setup) = with_household().await;
    let view = test
        .app
        .safe_to_spend
        .current(&setup.ctx, Scope::Household)
        .await
        .unwrap();
    assert_eq!(view.safe_to_spend, "0.00");
    assert_eq!(view.currency, "EUR");
    assert!(view.items.is_empty());
}
