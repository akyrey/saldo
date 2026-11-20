//! Phase 10 reports: savings rate, year over year, by counterparty and
//! category averages, against hand-computed figures. Today is 2026-11-13.

#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use chrono::NaiveDate;
use common::*;
use saldo_app::accounts::AccountView;
use saldo_app::categories::NewCategory;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::reports::{Interval, Metric, ReportQuery, YearOverYearQuery};
use saldo_app::scope::Scope;

const TODAY: (i32, u32, u32) = (2026, 11, 13);

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn today() -> NaiveDate {
    day(TODAY.0, TODAY.1, TODAY.2)
}

async fn book(
    test: &TestApp,
    ctx: &HouseholdCtx,
    on: NaiveDate,
    from: &AccountView,
    to: &AccountView,
    amount: &str,
    category: Option<&str>,
) {
    let mut input = split(1, from, to, amount);
    input.date = on;
    input.category_id = category.map(|c| c.parse().unwrap());
    test.app
        .transactions
        .create(ctx, Origin::Api, group(vec![input]))
        .await
        .unwrap();
}

struct World {
    test: TestApp,
    ctx: HouseholdCtx,
    checking: AccountView,
    shop: AccountView,
    salary: AccountView,
    food: String,
}

async fn world() -> World {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let shop = account(&test, &ctx, "Shop", "expense", None).await;
    let salary = account(&test, &ctx, "Salary", "revenue", None).await;
    let food = test
        .app
        .categories
        .create(
            &ctx,
            Origin::Api,
            NewCategory {
                name: "Food".into(),
                notes: None,
                color: None,
            },
        )
        .await
        .unwrap()
        .id;
    World {
        test,
        ctx,
        checking,
        shop,
        salary,
        food,
    }
}

#[tokio::test]
async fn cash_flow_carries_the_savings_rate() {
    let w = world().await;
    book(
        &w.test,
        &w.ctx,
        day(2026, 11, 1),
        &w.salary,
        &w.checking,
        "2000",
        None,
    )
    .await;
    book(
        &w.test,
        &w.ctx,
        day(2026, 11, 2),
        &w.checking,
        &w.shop,
        "500",
        None,
    )
    .await;
    book(
        &w.test,
        &w.ctx,
        day(2026, 10, 2),
        &w.checking,
        &w.shop,
        "40",
        None,
    )
    .await;
    let query = ReportQuery {
        from: day(2026, 10, 1),
        to: day(2026, 11, 30),
        interval: Interval::Month,
        scope: Scope::Household,
    };
    let report = w.test.app.reports.cash_flow(&w.ctx, &query).await.unwrap();
    let eur = &report.currencies[0];
    assert_eq!(
        eur.intervals[0].savings_rate_bp, None,
        "no income in October"
    );
    assert_eq!(eur.intervals[1].savings_rate_bp, Some(7500));
    // (2000 - 540) / 2000 = 73%.
    assert_eq!(eur.savings_rate_bp, Some(7300));
}

#[tokio::test]
async fn year_over_year_compares_the_same_months() {
    let w = world().await;
    let food = Some(w.food.as_str());
    book(
        &w.test,
        &w.ctx,
        day(2025, 11, 5),
        &w.checking,
        &w.shop,
        "100",
        food,
    )
    .await;
    book(
        &w.test,
        &w.ctx,
        day(2025, 12, 5),
        &w.checking,
        &w.shop,
        "50",
        food,
    )
    .await;
    book(
        &w.test,
        &w.ctx,
        day(2026, 11, 5),
        &w.checking,
        &w.shop,
        "150",
        food,
    )
    .await;
    book(
        &w.test,
        &w.ctx,
        day(2026, 11, 6),
        &w.checking,
        &w.shop,
        "7",
        None,
    )
    .await;
    let query = YearOverYearQuery {
        years: 2,
        metric: Metric::Expenses,
        category: Some(Some(w.food.clone())),
        scope: Scope::Household,
    };
    let report = w
        .test
        .app
        .reports
        .year_over_year(&w.ctx, today(), &query)
        .await
        .unwrap();
    let years = &report.currencies[0].years;
    assert_eq!((years[0].year, years[1].year), (2025, 2026));
    assert_eq!(years[0].total, "150.00");
    assert_eq!(years[0].change, None);
    let now = &years[1];
    assert_eq!(now.months[10].as_deref(), Some("150.00"));
    assert_eq!(now.months[11], None, "December is still to come");
    assert_eq!(now.month_change_bp[10], Some(5000));
    // January to November: 150 against 100 a year earlier.
    assert_eq!(now.total, "150.00");
    assert_eq!(now.change.as_deref(), Some("50.00"));
    assert_eq!(now.change_bp, Some(5000));

    let all = YearOverYearQuery {
        category: None,
        ..query.clone()
    };
    let report = w
        .test
        .app
        .reports
        .year_over_year(&w.ctx, today(), &all)
        .await
        .unwrap();
    assert_eq!(report.currencies[0].years[1].total, "157.00");
    let uncategorised = YearOverYearQuery {
        category: Some(None),
        ..query.clone()
    };
    let report = w
        .test
        .app
        .reports
        .year_over_year(&w.ctx, today(), &uncategorised)
        .await
        .unwrap();
    assert_eq!(report.category_id.as_deref(), Some("none"));
    assert_eq!(report.currencies[0].years[1].total, "7.00");

    let too_many = YearOverYearQuery { years: 6, ..query };
    assert!(
        w.test
            .app
            .reports
            .year_over_year(&w.ctx, today(), &too_many)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn by_counterparty_groups_by_the_other_account() {
    let w = world().await;
    let cafe = account(&w.test, &w.ctx, "Cafe", "expense", None).await;
    book(
        &w.test,
        &w.ctx,
        day(2026, 11, 1),
        &w.salary,
        &w.checking,
        "2000",
        None,
    )
    .await;
    book(
        &w.test,
        &w.ctx,
        day(2026, 11, 2),
        &w.checking,
        &w.shop,
        "80",
        None,
    )
    .await;
    book(
        &w.test,
        &w.ctx,
        day(2026, 11, 3),
        &w.checking,
        &cafe,
        "4.50",
        None,
    )
    .await;
    book(
        &w.test,
        &w.ctx,
        day(2026, 11, 4),
        &w.checking,
        &w.shop,
        "20",
        None,
    )
    .await;
    let query = ReportQuery {
        from: day(2026, 11, 1),
        to: day(2026, 11, 30),
        interval: Interval::Month,
        scope: Scope::Household,
    };
    let report = w
        .test
        .app
        .reports
        .by_counterparty(&w.ctx, &query)
        .await
        .unwrap();
    let items: Vec<_> = report.currencies[0]
        .items
        .iter()
        .map(|i| {
            (
                i.name.clone().unwrap(),
                i.income.as_str(),
                i.expenses.as_str(),
            )
        })
        .collect();
    assert_eq!(
        items,
        [
            ("Shop".to_string(), "0.00", "100.00"),
            ("Cafe".to_string(), "0.00", "4.50"),
            ("Salary".to_string(), "2000.00", "0.00"),
        ]
    );
}

#[tokio::test]
async fn category_averages_count_empty_months_as_zero() {
    let w = world().await;
    let food = Some(w.food.as_str());
    book(
        &w.test,
        &w.ctx,
        day(2026, 8, 10),
        &w.checking,
        &w.shop,
        "30",
        food,
    )
    .await;
    book(
        &w.test,
        &w.ctx,
        day(2026, 10, 10),
        &w.checking,
        &w.shop,
        "60",
        food,
    )
    .await;
    book(
        &w.test,
        &w.ctx,
        day(2026, 11, 2),
        &w.checking,
        &w.shop,
        "20",
        food,
    )
    .await;
    book(
        &w.test,
        &w.ctx,
        day(2026, 10, 11),
        &w.checking,
        &w.shop,
        "10",
        None,
    )
    .await;
    // Before the window: ignored.
    book(
        &w.test,
        &w.ctx,
        day(2026, 7, 31),
        &w.checking,
        &w.shop,
        "999",
        food,
    )
    .await;
    let report = w
        .test
        .app
        .reports
        .category_averages(&w.ctx, today(), 3, Scope::Household)
        .await
        .unwrap();
    assert_eq!(
        (report.from.as_str(), report.to.as_str()),
        ("2026-08-01", "2026-10-31")
    );
    let items = &report.currencies[0].items;
    assert_eq!(items[0].name.as_deref(), Some("Food"));
    assert_eq!(items[0].average, "30.00");
    assert_eq!(
        (items[0].min.as_str(), items[0].max.as_str()),
        ("0.00", "60.00")
    );
    assert_eq!(items[0].this_month, "20.00");
    assert_eq!(items[0].this_month_vs_average_bp, Some(-3333));
    assert_eq!(items[1].id, None);
    assert_eq!(items[1].average, "3.33");
    assert!(
        w.test
            .app
            .reports
            .category_averages(&w.ctx, today(), 2, Scope::Household)
            .await
            .is_err()
    );
}
