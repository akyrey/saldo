#![allow(clippy::unwrap_used)] // test helpers outside #[test] functions

mod common;

use chrono::{Datelike, NaiveDate};
use common::*;
use saldo_app::accounts::{AccountView, NewAccount};
use saldo_app::budgets::{NewBudget, NewBudgetLimit};
use saldo_app::categories::NewCategory;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::reports::{Interval, ReportQuery};
use saldo_app::scope::Scope;
use saldo_app::tags::NewTag;
use saldo_app::transactions::SplitInput;
use saldo_core::{BudgetId, CategoryId, TagId, UserId};

fn day(m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, m, d).unwrap()
}

fn query(from: NaiveDate, to: NaiveDate, interval: Interval, scope: Scope) -> ReportQuery {
    ReportQuery {
        from,
        to,
        interval,
        scope,
    }
}

/// Minor units as a two-decimal string, the way the reports print EUR and USD.
fn money(minor: i64) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    format!("{sign}{}.{:02}", minor.abs() / 100, minor.abs() % 100)
}

async fn scalar(test: &TestApp, sql: &str) -> i64 {
    let (n,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(sql.to_string()))
        .fetch_one(&test.app.pool)
        .await
        .unwrap();
    n
}

async fn owned(
    test: &TestApp,
    ctx: &HouseholdCtx,
    name: &str,
    account_type: &str,
    currency: Option<&str>,
    owner: Option<UserId>,
) -> AccountView {
    let input = NewAccount {
        name: name.into(),
        account_type: account_type.into(),
        currency: currency.map(str::to_string),
        owner_user_id: owner,
        ..NewAccount::default()
    };
    test.app
        .accounts
        .create(ctx, Origin::Api, input)
        .await
        .unwrap()
}

/// Moves a split built for November to the same day of month `m`.
fn on(mut s: SplitInput, m: u32) -> SplitInput {
    s.date = day(m, s.date.day0() + 1);
    s
}

struct Ledger {
    food: CategoryId,
    budget: BudgetId,
    member: UserId,
}

/// A small household ledger over October to December 2026.
async fn ledger(test: &TestApp, ctx: &HouseholdCtx) -> Ledger {
    let member = add_member(test, ctx, "partner@example.com").await.user_id;
    let checking = owned(test, ctx, "Checking", "asset", None, Some(ctx.user_id)).await;
    let savings = owned(test, ctx, "Savings", "asset", None, Some(ctx.user_id)).await;
    let card = owned(test, ctx, "Card", "liability", None, Some(member)).await;
    let wallet = owned(test, ctx, "Wallet", "asset", Some("USD"), Some(ctx.user_id)).await;
    let groceries = owned(test, ctx, "Groceries", "expense", None, None).await;
    let rent = owned(test, ctx, "Rent", "expense", None, None).await;
    let shop = owned(test, ctx, "US shop", "expense", Some("USD"), None).await;
    let employer = owned(test, ctx, "Employer", "revenue", None, None).await;
    let food = test
        .app
        .categories
        .create(
            ctx,
            Origin::Api,
            NewCategory {
                name: "Food".into(),
                notes: None,
                color: None,
            },
        )
        .await
        .unwrap();
    let food: CategoryId = food.id.parse().unwrap();
    let trip = test
        .app
        .tags
        .create(
            ctx,
            Origin::Api,
            NewTag {
                name: "trip".into(),
                description: None,
            },
        )
        .await
        .unwrap();
    let trip: TagId = trip.id.parse().unwrap();
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
    let budget: BudgetId = budget.id.parse().unwrap();
    test.app
        .budgets
        .create_limit(
            ctx,
            Origin::Api,
            budget,
            NewBudgetLimit {
                start_date: day(11, 1),
                end_date: day(11, 30),
                amount: "300".into(),
                currency: None,
                notes: None,
            },
        )
        .await
        .unwrap();

    let mut shop_split = split(2, &checking, &groceries, "42.50");
    shop_split.category_id = Some(food);
    shop_split.tag_ids = vec![trip];
    shop_split.budget_id = Some(budget);
    let mut card_split = split(3, &card, &groceries, "17.25");
    card_split.category_id = Some(food);
    let mut usd = split(12, &wallet, &shop, "10.00");
    usd.tag_ids = vec![trip];
    for splits in [
        vec![on(split(30, &employer, &checking, "2500.00"), 10)],
        vec![shop_split, card_split],
        vec![split(5, &checking, &savings, "100.00")],
        vec![split(10, &employer, &checking, "2500.00")],
        vec![split(10, &employer, &wallet, "50.00")],
        vec![usd],
        vec![on(split(1, &checking, &rent, "800.00"), 12)],
    ] {
        test.app
            .transactions
            .create(ctx, Origin::Api, group(splits))
            .await
            .unwrap();
    }
    Ledger {
        food,
        budget,
        member,
    }
}

const NOV: &str = "date >= '2026-11-01' AND date <= '2026-11-30'";

#[tokio::test]
async fn cash_flow_totals_match_an_independent_query() {
    let (test, setup) = with_household().await;
    let ctx = &setup.ctx;
    ledger(&test, ctx).await;

    let report = test
        .app
        .reports
        .cash_flow(
            ctx,
            &query(day(11, 1), day(11, 30), Interval::Month, Scope::Household),
        )
        .await
        .unwrap();

    let sum = |kind: &str, currency: &str| {
        format!(
            "SELECT COALESCE(SUM(amount_minor), 0) FROM splits WHERE {NOV} AND kind = '{kind}' AND currency_code = '{currency}'"
        )
    };
    let codes: Vec<_> = report
        .currencies
        .iter()
        .map(|c| c.currency.as_str())
        .collect();
    assert_eq!(codes, ["EUR", "USD"], "primary currency first");
    for currency in &report.currencies {
        let income = scalar(&test, &sum("deposit", &currency.currency)).await;
        let expenses = scalar(&test, &sum("withdrawal", &currency.currency)).await;
        assert_eq!(currency.income, money(income));
        assert_eq!(currency.expenses, money(expenses));
        assert_eq!(currency.net, money(income - expenses));
        assert_eq!(currency.intervals.len(), 1);
        assert_eq!(currency.intervals[0].expenses, currency.expenses);
    }
    // The transfer to savings is neither income nor expense.
    assert_eq!(report.currencies[0].income, "2500.00");
    assert_eq!(report.currencies[0].expenses, "59.75");

    let weekly = test
        .app
        .reports
        .cash_flow(
            ctx,
            &query(day(11, 1), day(11, 30), Interval::Week, Scope::Household),
        )
        .await
        .unwrap();
    let eur = &weekly.currencies[0];
    assert_eq!(eur.intervals.len(), 6);
    assert_eq!(
        (
            eur.intervals[0].start.as_str(),
            eur.intervals[0].end.as_str()
        ),
        ("2026-11-01", "2026-11-01")
    );
    assert_eq!(eur.intervals[1].expenses, "59.75");
    assert_eq!(eur.intervals[2].income, "2500.00");
}

#[tokio::test]
async fn scope_counts_only_the_members_accounts() {
    let (test, setup) = with_household().await;
    let ctx = &setup.ctx;
    let ledger = ledger(&test, ctx).await;
    let mine = test
        .app
        .reports
        .cash_flow(
            ctx,
            &query(
                day(11, 1),
                day(11, 30),
                Interval::Month,
                Scope::User(ledger.member),
            ),
        )
        .await
        .unwrap();
    assert_eq!(mine.currencies.len(), 1);
    assert_eq!(
        (
            mine.currencies[0].income.as_str(),
            mine.currencies[0].expenses.as_str()
        ),
        ("0.00", "17.25")
    );
}

#[tokio::test]
async fn categories_tags_and_budgets_break_spending_down() {
    let (test, setup) = with_household().await;
    let ctx = &setup.ctx;
    let ledger = ledger(&test, ctx).await;
    let nov = query(day(11, 1), day(11, 30), Interval::Month, Scope::Household);

    let by_category = test.app.reports.by_category(ctx, &nov).await.unwrap();
    let eur = &by_category.currencies[0];
    let food = eur
        .items
        .iter()
        .find(|i| i.id == Some(ledger.food.to_string()))
        .unwrap();
    let expected = scalar(&test, &format!("SELECT SUM(amount_minor) FROM splits WHERE {NOV} AND kind = 'withdrawal' AND category_id = '{}'", ledger.food)).await;
    assert_eq!(
        (food.name.as_deref(), food.expenses.clone()),
        (Some("Food"), money(expected))
    );
    let none = eur.items.iter().find(|i| i.id.is_none()).unwrap();
    assert_eq!(none.income, "2500.00");
    assert_eq!(eur.items[0].expenses, "59.75", "largest expenses first");

    let by_tag = test.app.reports.by_tag(ctx, &nov).await.unwrap();
    let tagged: Vec<_> = by_tag
        .currencies
        .iter()
        .map(|c| {
            (
                c.currency.as_str(),
                c.items
                    .iter()
                    .find(|i| i.name.as_deref() == Some("trip"))
                    .unwrap()
                    .expenses
                    .as_str(),
            )
        })
        .collect();
    assert_eq!(tagged, [("EUR", "42.50"), ("USD", "10.00")]);

    let budgets = test.app.reports.budget_vs_actual(ctx, &nov).await.unwrap();
    let items = &budgets.currencies[0].items;
    let food = &items[0];
    assert_eq!(food.budget_id, Some(ledger.budget.to_string()));
    assert_eq!(
        (food.budgeted.as_str(), food.spent.as_str()),
        ("300.00", "42.50")
    );
    let without = items.last().unwrap();
    assert_eq!(
        (without.budget_id.clone(), without.spent.as_str()),
        (None, "17.25")
    );
}

#[tokio::test]
async fn net_worth_matches_account_balances() {
    let (test, setup) = with_household().await;
    let ctx = &setup.ctx;
    ledger(&test, ctx).await;

    let report = test
        .app
        .reports
        .net_worth(
            ctx,
            &query(day(11, 1), day(12, 31), Interval::Month, Scope::Household),
        )
        .await
        .unwrap();
    // Independently: incoming minus outgoing of every asset and liability
    // account in net worth, per account currency, up to a date.
    let balance = |currency: &str, until: &str| {
        format!(
            "SELECT COALESCE(SUM(CASE WHEN s.destination_account_id = a.id THEN s.amount_minor ELSE -s.amount_minor END), 0)
             FROM accounts a JOIN splits s ON a.id IN (s.source_account_id, s.destination_account_id)
             WHERE a.type IN ('asset', 'liability') AND a.include_net_worth = 1
               AND a.currency_code = '{currency}' AND s.date <= '{until}'"
        )
    };
    let eur = &report.currencies[0];
    assert_eq!(
        eur.opening,
        money(scalar(&test, &balance("EUR", "2026-10-31")).await)
    );
    assert_eq!(
        eur.intervals[0].value,
        money(scalar(&test, &balance("EUR", "2026-11-30")).await)
    );
    assert_eq!(
        eur.intervals[1].value,
        money(scalar(&test, &balance("EUR", "2026-12-31")).await)
    );
    assert_eq!(eur.intervals[1].change, "-800.00");
    assert_eq!(report.currencies[1].intervals[0].value, "40.00");

    let bad = query(day(11, 2), day(11, 1), Interval::Day, Scope::Household);
    assert!(matches!(
        test.app.reports.net_worth(ctx, &bad).await,
        Err(AppError::Invalid { .. })
    ));
}
