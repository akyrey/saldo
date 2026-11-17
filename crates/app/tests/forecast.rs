#![allow(clippy::unwrap_used)] // test helpers outside #[test] functions

mod common;

use chrono::NaiveDate;
use common::*;
use saldo_app::accounts::AccountView;
use saldo_app::bills::NewBill;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::recurrences::{RecurrenceInput, RepetitionInput, TemplateSplitInput};
use saldo_app::reports::{ForecastQuery, ForecastReport, Interval};
use saldo_app::scope::Scope;
use saldo_core::BillId;

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn bill(name: &str, first: NaiveDate, freq: &str, min: &str, max: &str) -> NewBill {
    NewBill {
        name: name.into(),
        amount_min: min.into(),
        amount_max: max.into(),
        currency: None,
        first_due_date: first,
        end_date: None,
        repeat_freq: freq.into(),
        skip: None,
        remind_days_before: None,
        remind_every_days: None,
        active: None,
        sort_order: None,
        notes: None,
    }
}

async fn create_bill(test: &TestApp, ctx: &HouseholdCtx, input: NewBill) -> BillId {
    let view = test
        .app
        .bills
        .create(ctx, Origin::Api, input)
        .await
        .unwrap();
    view.id.parse().unwrap()
}

/// Rent on the 1st of every month from December, paying the rent bill.
fn rent(checking: &AccountView, landlord: &AccountView, rent_bill: BillId) -> RecurrenceInput {
    RecurrenceInput {
        title: "Rent".into(),
        description: None,
        first_date: day(2026, 12, 1),
        repeat_until: None,
        max_occurrences: None,
        apply_rules: None,
        active: None,
        notes: None,
        repetitions: vec![RepetitionInput {
            kind: "monthly".into(),
            moment: "1".into(),
            skip: 0,
            weekend_policy: None,
        }],
        splits: vec![TemplateSplitInput {
            description: "Rent".into(),
            source_id: account_id(checking),
            destination_id: account_id(landlord),
            amount: "800.00".into(),
            currency: None,
            foreign_amount: None,
            foreign_currency: None,
            category_id: None,
            budget_id: None,
            bill_id: Some(rent_bill),
            piggy_bank_id: None,
            tag_ids: Vec::new(),
        }],
    }
}

/// Today is 2026-11-13 (the test clock). Checking gets 2000.00 on 1
/// November and pays the phone bill (25.00) on the 5th; 100.00 is already
/// entered for the 20th. Rent (800.00) recurs from December and pays the rent
/// bill, which must not count twice. Insurance (120.00 at most, yearly from
/// 15 January) was never paid, so it has no account.
async fn household() -> (TestApp, HouseholdCtx, AccountView) {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let salary = account(&test, &ctx, "Employer", "revenue", None).await;
    let shop = account(&test, &ctx, "Shop", "expense", None).await;
    let landlord = account(&test, &ctx, "Landlord", "expense", None).await;
    let telco = account(&test, &ctx, "Telco", "expense", None).await;

    let phone = create_bill(
        &test,
        &ctx,
        bill("Phone", day(2026, 11, 5), "monthly", "20", "30"),
    )
    .await;
    let rent_bill = create_bill(
        &test,
        &ctx,
        bill("Rent", day(2026, 12, 1), "monthly", "800", "800"),
    )
    .await;
    let insurance = bill("Insurance", day(2027, 1, 15), "yearly", "100", "120");
    create_bill(&test, &ctx, insurance).await;

    let mut phone_payment = split(5, &checking, &telco, "25.00");
    phone_payment.bill_id = Some(phone);
    for s in [
        split(1, &salary, &checking, "2000.00"),
        phone_payment,
        split(20, &checking, &shop, "100.00"),
    ] {
        test.app
            .transactions
            .create(&ctx, Origin::Api, group(vec![s]))
            .await
            .unwrap();
    }
    test.app
        .recurrences
        .create(&ctx, Origin::Api, rent(&checking, &landlord, rent_bill))
        .await
        .unwrap();
    (test, ctx, checking)
}

async fn forecast(test: &TestApp, ctx: &HouseholdCtx, query: ForecastQuery) -> ForecastReport {
    test.app
        .reports
        .forecast(ctx, day(2026, 11, 13), &query)
        .await
        .unwrap()
}

fn monthly_to_january() -> ForecastQuery {
    ForecastQuery {
        to: day(2027, 1, 31),
        interval: Interval::Month,
        scope: Scope::Household,
        account_id: None,
    }
}

#[tokio::test]
async fn projects_transactions_recurrences_and_bills() {
    let (test, ctx, _) = household().await;
    let report = forecast(&test, &ctx, monthly_to_january()).await;

    assert_eq!(report.range.from, "2026-11-13");
    let eur = &report.currencies[0];
    assert_eq!(eur.currency, "EUR");
    assert_eq!(eur.start_balance, "1975.00");
    let points: Vec<_> = eur
        .points
        .iter()
        .map(|p| (p.date.as_str(), p.balance.as_str()))
        .collect();
    assert_eq!(
        points,
        vec![
            ("2026-11-30", "1875.00"),
            ("2026-12-31", "1045.00"),
            ("2027-01-31", "95.00"),
        ]
    );
    assert_eq!(
        (eur.lowest.date.as_str(), eur.lowest.balance.as_str()),
        ("2027-01-15", "95.00")
    );
    let items: Vec<_> = eur
        .items
        .iter()
        .map(|i| {
            (
                i.date.as_str(),
                i.source.as_str(),
                i.title.as_str(),
                i.amount.as_str(),
            )
        })
        .collect();
    assert_eq!(
        items,
        vec![
            ("2026-11-20", "transaction", "Checking to Shop", "-100.00"),
            ("2026-12-01", "recurrence", "Rent", "-800.00"),
            ("2026-12-05", "bill", "Phone", "-30.00"),
            ("2027-01-01", "recurrence", "Rent", "-800.00"),
            ("2027-01-05", "bill", "Phone", "-30.00"),
            ("2027-01-15", "bill", "Insurance", "-120.00"),
        ]
    );
    assert!(eur.items[5].account_id.is_none());
}

#[tokio::test]
async fn one_account_leaves_out_bills_nobody_paid() {
    let (test, ctx, checking) = household().await;
    let query = ForecastQuery {
        account_id: Some(account_id(&checking)),
        ..monthly_to_january()
    };
    let report = forecast(&test, &ctx, query).await;
    let last = report.currencies[0].points.last().unwrap();
    assert_eq!(last.balance, "215.00");
}

#[tokio::test]
async fn the_span_and_the_account_are_checked() {
    let (test, ctx, _) = household().await;
    let expense = account(&test, &ctx, "Cafe", "expense", None).await;
    let cases = [
        (
            "to",
            ForecastQuery {
                to: day(2026, 11, 13),
                ..monthly_to_january()
            },
        ),
        (
            "to",
            ForecastQuery {
                to: day(2028, 11, 14),
                ..monthly_to_january()
            },
        ),
        (
            "interval",
            ForecastQuery {
                interval: Interval::Year,
                ..monthly_to_january()
            },
        ),
        (
            "account_id",
            ForecastQuery {
                account_id: Some(account_id(&expense)),
                ..monthly_to_january()
            },
        ),
    ];
    for (field, query) in cases {
        let err = test
            .app
            .reports
            .forecast(&ctx, day(2026, 11, 13), &query)
            .await
            .unwrap_err();
        assert!(
            matches!(err, AppError::Invalid { field: ref f, .. } if f == field),
            "{field}: {err:?}"
        );
    }
    // Exactly 24 months ahead is allowed.
    let longest = ForecastQuery {
        to: day(2028, 11, 13),
        interval: Interval::Week,
        ..monthly_to_january()
    };
    assert!(
        test.app
            .reports
            .forecast(&ctx, day(2026, 11, 13), &longest)
            .await
            .is_ok()
    );
}
