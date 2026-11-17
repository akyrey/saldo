#![allow(clippy::unwrap_used)] // test helpers outside #[test] functions

mod common;

use chrono::{NaiveDate, TimeDelta};
use common::*;
use saldo_app::budgets::{
    AutoBudgetInput, BudgetFilter, BudgetLimitUpdate, BudgetUpdate, NewBudget, NewBudgetLimit,
};
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::page::PageRequest;
use saldo_core::{BudgetId, GroupId};

fn page() -> PageRequest {
    PageRequest::new(None, Some(200)).unwrap()
}

fn monthly(auto_type: &str, amount: &str) -> AutoBudgetInput {
    AutoBudgetInput {
        auto_type: auto_type.into(),
        period: "monthly".into(),
        amount: amount.into(),
        currency: None,
    }
}

async fn budget(test: &TestApp, ctx: &HouseholdCtx, name: &str) -> BudgetId {
    let input = NewBudget {
        name: name.into(),
        ..NewBudget::default()
    };
    let view = test
        .app
        .budgets
        .create(ctx, Origin::Api, input)
        .await
        .unwrap();
    view.id.parse().unwrap()
}

fn limit(start: NaiveDate, end: NaiveDate, amount: &str) -> NewBudgetLimit {
    NewBudgetLimit {
        start_date: start,
        end_date: end,
        amount: amount.into(),
        currency: None,
        notes: None,
    }
}

/// A withdrawal of `amount` on `day` of November 2026, booked on `budget`.
async fn spend(
    test: &TestApp,
    ctx: &HouseholdCtx,
    day: u32,
    amount: &str,
    budget: Option<BudgetId>,
) -> GroupId {
    // One pair of accounts per day keeps the names unique.
    let checking = account(test, ctx, &format!("Checking {day}"), "asset", None).await;
    let shop = account(test, ctx, &format!("Shop {day}"), "expense", None).await;
    let mut s = split(day, &checking, &shop, amount);
    s.budget_id = budget;
    let view = test
        .app
        .transactions
        .create(ctx, Origin::Api, group(vec![s]))
        .await
        .unwrap();
    view.id.parse().unwrap()
}

#[tokio::test]
async fn budget_lifecycle_with_events() {
    let (test, setup) = with_household().await;
    let ctx = &setup.ctx;
    let input = NewBudget {
        name: " Groceries ".into(),
        notes: Some("weekly shop".into()),
        sort_order: 2,
        ..NewBudget::default()
    };
    let created = test
        .app
        .budgets
        .create(ctx, Origin::Api, input)
        .await
        .unwrap();
    assert_eq!(created.name, "Groceries");
    assert!(created.active && created.auto_budget.is_none());
    assert_eq!(last_event(&test, ctx).await.event_type, "budget.created");
    let id: BudgetId = created.id.parse().unwrap();

    let dup = NewBudget {
        name: "Groceries".into(),
        ..NewBudget::default()
    };
    let err = test
        .app
        .budgets
        .create(ctx, Origin::Api, dup)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)));
    let blank = NewBudget::default();
    let err = test
        .app
        .budgets
        .create(ctx, Origin::Api, blank)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Invalid { ref field, .. } if field == "name"));

    let update = BudgetUpdate {
        name: Some("Food".into()),
        active: Some(false),
        notes: Some(None),
        ..BudgetUpdate::default()
    };
    let updated = test
        .app
        .budgets
        .update(ctx, Origin::Api, id, update)
        .await
        .unwrap();
    assert_eq!(
        (updated.name.as_str(), updated.active, updated.notes.clone()),
        ("Food", false, None)
    );
    let event = last_event(&test, ctx).await;
    assert_eq!(event.event_type, "budget.updated");
    assert_eq!(event.payload["data"]["name"], "Food");

    let active = BudgetFilter { active: Some(true) };
    assert_eq!(
        test.app
            .budgets
            .list(ctx, &active, page())
            .await
            .unwrap()
            .total,
        0
    );
    let all = test
        .app
        .budgets
        .list(ctx, &BudgetFilter::default(), page())
        .await
        .unwrap();
    assert_eq!(all.items, vec![updated]);

    test.app.budgets.delete(ctx, Origin::Api, id).await.unwrap();
    assert_eq!(last_event(&test, ctx).await.event_type, "budget.deleted");
    let err = test.app.budgets.get(ctx, id).await.unwrap_err();
    assert!(matches!(err, AppError::NotFound));
}

#[tokio::test]
async fn an_auto_budget_creates_the_current_period_limit() {
    let (test, setup) = with_household().await;
    let ctx = &setup.ctx;
    let input = NewBudget {
        name: "Groceries".into(),
        auto_budget: Some(monthly("reset", "300")),
        ..NewBudget::default()
    };
    let view = test
        .app
        .budgets
        .create(ctx, Origin::Api, input)
        .await
        .unwrap();
    let auto = view.auto_budget.clone().unwrap();
    assert_eq!(
        (
            auto.auto_type.as_str(),
            auto.period.as_str(),
            auto.amount.as_str(),
            auto.currency.as_str()
        ),
        ("reset", "monthly", "300.00", "EUR")
    );
    let id: BudgetId = view.id.parse().unwrap();
    let limits = test
        .app
        .budgets
        .list_limits(ctx, id, None, None, page())
        .await
        .unwrap();
    assert_eq!(limits.total, 1);
    let first = &limits.items[0];
    assert_eq!(
        (
            first.start_date.as_str(),
            first.end_date.as_str(),
            first.amount.as_str()
        ),
        ("2026-11-01", "2026-11-30", "300.00")
    );
    assert_eq!(
        last_event(&test, ctx).await.event_type,
        "budget_limit.created"
    );

    // Running the generator again in the same period adds nothing.
    let today = test.app.identity.household_today(ctx).await.unwrap();
    assert_eq!(
        test.app
            .budgets
            .create_auto_limits(ctx, today)
            .await
            .unwrap(),
        0
    );

    for (field, bad) in [
        (
            "auto_budget.type",
            AutoBudgetInput {
                auto_type: "adjusted".into(),
                ..monthly("reset", "1")
            },
        ),
        (
            "auto_budget.period",
            AutoBudgetInput {
                period: "fortnightly".into(),
                ..monthly("reset", "1")
            },
        ),
        ("auto_budget.amount", monthly("reset", "0")),
        (
            "auto_budget.currency",
            AutoBudgetInput {
                currency: Some("XXX".into()),
                ..monthly("reset", "1")
            },
        ),
    ] {
        let update = BudgetUpdate {
            auto_budget: Some(Some(bad)),
            ..BudgetUpdate::default()
        };
        let err = test
            .app
            .budgets
            .update(ctx, Origin::Api, id, update)
            .await
            .unwrap_err();
        assert!(
            matches!(err, AppError::Invalid { field: ref f, .. } if f == field),
            "{field}: {err:?}"
        );
    }
    let remove = BudgetUpdate {
        auto_budget: Some(None),
        ..BudgetUpdate::default()
    };
    let view = test
        .app
        .budgets
        .update(ctx, Origin::Api, id, remove)
        .await
        .unwrap();
    assert!(view.auto_budget.is_none());
}

/// The December limit of a budget with `carry` and 100.00 a month, after
/// `spent` in November; returns (amount, carried, available).
async fn december(test: &TestApp, ctx: &HouseholdCtx, id: BudgetId) -> (String, String, String) {
    let limits = test
        .app
        .budgets
        .list_limits(ctx, id, None, None, page())
        .await
        .unwrap();
    let december = &limits.items[0];
    assert_eq!(december.start_date, "2026-12-01");
    (
        december.amount.clone(),
        december.carried.clone(),
        december.available.clone(),
    )
}

#[tokio::test]
async fn carry_is_computed_when_limits_are_read() {
    let (test, setup) = with_household().await;
    let ctx = &setup.ctx;
    let input = NewBudget {
        name: "Fun".into(),
        auto_budget: Some(monthly("reset", "100")),
        carry: "leftover".into(),
        ..NewBudget::default()
    };
    let view = test
        .app
        .budgets
        .create(ctx, Origin::Api, input)
        .await
        .unwrap();
    assert_eq!(view.carry, "leftover");
    let id: BudgetId = view.id.parse().unwrap();
    spend(&test, ctx, 20, "40.00", Some(id)).await;

    test.clock.advance(TimeDelta::days(30)); // 2026-12-13
    let today = test.app.identity.household_today(ctx).await.unwrap();
    test.app
        .budgets
        .create_auto_limits(ctx, today)
        .await
        .unwrap();
    // The stored amount stays the base; the carry is added when read.
    assert_eq!(
        december(&test, ctx, id).await,
        ("100.00".into(), "60.00".into(), "160.00".into())
    );

    // A late November payment changes December without touching its row.
    spend(&test, ctx, 25, "50.00", Some(id)).await;
    assert_eq!(
        last_event(&test, ctx).await.event_type,
        "transaction.created"
    );
    assert_eq!(december(&test, ctx, id).await.1, "10.00");

    // Overspending is forgotten with `leftover` and carried with `all`.
    spend(&test, ctx, 26, "30.00", Some(id)).await;
    assert_eq!(december(&test, ctx, id).await.1, "0.00");
    let update = BudgetUpdate {
        carry: Some("all".into()),
        ..BudgetUpdate::default()
    };
    let view = test
        .app
        .budgets
        .update(ctx, Origin::Api, id, update)
        .await
        .unwrap();
    assert_eq!(view.carry, "all");
    assert_eq!(last_event(&test, ctx).await.event_type, "budget.updated");
    assert_eq!(
        december(&test, ctx, id).await,
        ("100.00".into(), "-20.00".into(), "80.00".into())
    );
}

#[tokio::test]
async fn hand_made_limits_carry_and_bad_values_are_rejected() {
    let (test, setup) = with_household().await;
    let ctx = &setup.ctx;
    let id = budget(&test, ctx, "Trips").await;
    let update = BudgetUpdate {
        carry: Some("leftover".into()),
        ..BudgetUpdate::default()
    };
    test.app
        .budgets
        .update(ctx, Origin::Api, id, update)
        .await
        .unwrap();
    let december_day = |day| NaiveDate::from_ymd_opt(2026, 12, day).unwrap();
    for (start, end) in [(date(1), date(30)), (december_day(1), december_day(31))] {
        test.app
            .budgets
            .create_limit(ctx, Origin::Api, id, limit(start, end, "200"))
            .await
            .unwrap();
    }
    spend(&test, ctx, 3, "50.00", Some(id)).await;
    assert_eq!(
        december(&test, ctx, id).await,
        ("200.00".into(), "150.00".into(), "350.00".into())
    );

    for (field, update) in [
        (
            "carry",
            BudgetUpdate {
                carry: Some("sometimes".into()),
                ..BudgetUpdate::default()
            },
        ),
        (
            "auto_budget.type",
            BudgetUpdate {
                auto_budget: Some(Some(monthly("rollover", "1"))),
                ..BudgetUpdate::default()
            },
        ),
    ] {
        let err = test
            .app
            .budgets
            .update(ctx, Origin::Api, id, update)
            .await
            .unwrap_err();
        assert!(
            matches!(err, AppError::Invalid { field: ref f, .. } if f == field),
            "{field}: {err:?}"
        );
    }
}

/// Rollover budgets from before Phase 9 stored the carry in each limit.
#[tokio::test]
async fn the_carry_migration_resets_rollover_limits_to_the_base() {
    use saldo_app::db::{MIGRATOR, connect_in_memory};
    const BEFORE: i64 = 20261115183605;
    let pool = connect_in_memory().await.unwrap();
    MIGRATOR.run_to(BEFORE, &pool).await.unwrap();
    for sql in [
        "INSERT INTO households (id, name, primary_currency, created_at, updated_at)
         VALUES ('h', 'Home', 'EUR', 't', 't')",
        "INSERT INTO budgets (id, household_id, name, auto_type, auto_period, auto_amount_minor,
                              auto_currency_code, created_at, updated_at)
         VALUES ('roll', 'h', 'Fun', 'rollover', 'monthly', 10000, 'EUR', 't', 't'),
                ('plain', 'h', 'Food', 'reset', 'monthly', 10000, 'EUR', 't', 't')",
        "INSERT INTO budget_limits (id, household_id, budget_id, start_date, end_date, amount_minor,
                                    currency_code, created_at, updated_at)
         VALUES ('a', 'h', 'roll', '2026-10-01', '2026-10-31', 16000, 'EUR', 't', 't'),
                ('b', 'h', 'roll', '2026-11-01', '2026-11-30', 10000, 'EUR', 't', 't'),
                ('c', 'h', 'roll', '2026-11-01', '2026-11-30', 5000, 'USD', 't', 't'),
                ('d', 'h', 'plain', '2026-11-01', '2026-11-30', 25000, 'EUR', 't', 't')",
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    MIGRATOR.run(&pool).await.unwrap();

    let budgets: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, auto_type, carry FROM budgets ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        budgets,
        vec![
            ("plain".into(), "reset".into(), "none".into()),
            ("roll".into(), "reset".into(), "leftover".into()),
        ]
    );
    let amounts: Vec<(String, i64)> =
        sqlx::query_as("SELECT id, amount_minor FROM budget_limits ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    // The inflated EUR limit goes back to the base; USD and other budgets stay.
    assert_eq!(
        amounts,
        vec![
            ("a".into(), 10000),
            ("b".into(), 10000),
            ("c".into(), 5000),
            ("d".into(), 25000)
        ]
    );
}

#[tokio::test]
async fn limits_measure_withdrawals_booked_on_the_budget() {
    let (test, setup) = with_household().await;
    let ctx = &setup.ctx;
    let id = budget(&test, ctx, "Groceries").await;
    let other = budget(&test, ctx, "Other").await;
    spend(&test, ctx, 3, "100.00", Some(id)).await;
    spend(&test, ctx, 4, "25.50", Some(id)).await;
    spend(&test, ctx, 5, "999.00", Some(other)).await;
    spend(&test, ctx, 6, "999.00", None).await;

    let created = test
        .app
        .budgets
        .create_limit(ctx, Origin::Api, id, limit(date(1), date(30), "300"))
        .await
        .unwrap();
    assert_eq!(
        (
            created.amount.as_str(),
            created.spent.as_str(),
            created.available.as_str(),
            created.currency.as_str()
        ),
        ("300.00", "125.50", "174.50", "EUR")
    );
    assert_eq!(
        last_event(&test, ctx).await.event_type,
        "budget_limit.created"
    );

    let overlap = test
        .app
        .budgets
        .create_limit(ctx, Origin::Api, id, limit(date(30), date(30), "1"))
        .await
        .unwrap_err();
    assert!(matches!(overlap, AppError::Conflict(_)));
    let reversed = test
        .app
        .budgets
        .create_limit(ctx, Origin::Api, id, limit(date(30), date(1), "1"))
        .await
        .unwrap_err();
    assert!(matches!(reversed, AppError::Invalid { ref field, .. } if field == "end_date"));
    let zero = test
        .app
        .budgets
        .create_limit(ctx, Origin::Api, other, limit(date(1), date(2), "0"))
        .await
        .unwrap_err();
    assert!(matches!(zero, AppError::Invalid { ref field, .. } if field == "amount"));

    let update = BudgetLimitUpdate {
        end_date: Some(date(3)),
        amount: Some("50".into()),
        ..BudgetLimitUpdate::default()
    };
    let updated = test
        .app
        .budgets
        .update_limit(ctx, Origin::Api, &created.id, update)
        .await
        .unwrap();
    assert_eq!(
        (updated.spent.as_str(), updated.available.as_str()),
        ("100.00", "-50.00")
    );
    assert_eq!(
        last_event(&test, ctx).await.event_type,
        "budget_limit.updated"
    );

    let in_range = test
        .app
        .budgets
        .list_limits(ctx, id, Some(date(4)), Some(date(30)), page())
        .await
        .unwrap();
    assert_eq!(in_range.total, 0);
    test.app
        .budgets
        .delete_limit(ctx, Origin::Api, &created.id)
        .await
        .unwrap();
    assert_eq!(
        last_event(&test, ctx).await.event_type,
        "budget_limit.deleted"
    );
    let gone = test
        .app
        .budgets
        .get_limit(ctx, &created.id)
        .await
        .unwrap_err();
    assert!(matches!(gone, AppError::NotFound));
}

#[tokio::test]
async fn only_withdrawals_take_a_budget_of_the_household() {
    let (test, setup) = with_household().await;
    let ctx = &setup.ctx;
    let id = budget(&test, ctx, "Groceries").await;
    let checking = account(&test, ctx, "Checking", "asset", None).await;
    let salary = account(&test, ctx, "Employer", "revenue", None).await;
    let mut deposit = split(1, &salary, &checking, "10.00");
    deposit.budget_id = Some(id);
    let err = test
        .app
        .transactions
        .create(ctx, Origin::Api, group(vec![deposit]))
        .await
        .unwrap_err();
    assert!(
        matches!(err, AppError::Invalid { ref field, .. } if field == "splits[0].budget_id"),
        "{err:?}"
    );

    let other_ctx = other_household(&test).await;
    let foreign = budget(&test, &other_ctx, "Theirs").await;
    let err = spend_err(&test, ctx, foreign).await;
    assert!(
        matches!(err, AppError::Validation(ref e) if e[0].code() == "unknown_reference"),
        "{err:?}"
    );
    assert!(matches!(
        test.app.budgets.get(ctx, foreign).await,
        Err(AppError::NotFound)
    ));
    let list = test
        .app
        .budgets
        .list_limits(ctx, foreign, None, None, page())
        .await;
    assert!(matches!(list, Err(AppError::NotFound)));
}

async fn spend_err(test: &TestApp, ctx: &HouseholdCtx, budget: BudgetId) -> AppError {
    let checking = account(test, ctx, "Wallet", "asset", None).await;
    let shop = account(test, ctx, "Kiosk", "expense", None).await;
    let mut s = split(1, &checking, &shop, "1.00");
    s.budget_id = Some(budget);
    test.app
        .transactions
        .create(ctx, Origin::Api, group(vec![s]))
        .await
        .unwrap_err()
}

#[tokio::test]
async fn deleting_a_budget_clears_its_splits_and_limits() {
    let (test, setup) = with_household().await;
    let ctx = &setup.ctx;
    let id = budget(&test, ctx, "Groceries").await;
    let group_id = spend(&test, ctx, 3, "10.00", Some(id)).await;
    test.app
        .budgets
        .create_limit(ctx, Origin::Api, id, limit(date(1), date(30), "300"))
        .await
        .unwrap();

    test.app.budgets.delete(ctx, Origin::Api, id).await.unwrap();

    let group = test.app.transactions.get(ctx, group_id).await.unwrap();
    assert_eq!(group.splits[0].budget_id, None);
    assert_eq!(count(&test, "budget_limits").await, 0);
    assert_eq!(count(&test, "budgets").await, 0);
}
