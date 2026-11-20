#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use chrono::NaiveDate;
use common::*;
use saldo_app::accounts::AccountView;
use saldo_app::bills::{BillHistoryView, NewBill};
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::transactions::SplitInput;
use saldo_core::{BillId, BillPeriodId};

// The test clock's today is 2026-11-13.

fn day(text: &str) -> NaiveDate {
    text.parse().unwrap()
}

fn gym(first: &str, end: Option<&str>) -> NewBill {
    NewBill {
        name: format!("Gym {first}"),
        amount_min: "39".into(),
        amount_max: "45".into(),
        currency: None,
        first_due_date: day(first),
        end_date: end.map(day),
        repeat_freq: "monthly".into(),
        skip: None,
        remind_days_before: None,
        remind_every_days: None,
        active: None,
        sort_order: None,
        notes: None,
    }
}

struct Fixture {
    test: TestApp,
    ctx: HouseholdCtx,
    checking: AccountView,
    gym: AccountView,
}

async fn fixture() -> Fixture {
    let (test, setup) = with_household().await;
    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let gym = account(&test, &setup.ctx, "Gym", "expense", None).await;
    Fixture {
        test,
        ctx: setup.ctx,
        checking,
        gym,
    }
}

impl Fixture {
    async fn bill(&self, first: &str, end: Option<&str>) -> BillId {
        let view = self
            .test
            .app
            .bills
            .create(&self.ctx, Origin::Api, gym(first, end))
            .await
            .unwrap();
        view.id.parse().unwrap()
    }

    /// Creates a one-split group and returns the split's ID.
    async fn pay(&self, split: SplitInput) -> String {
        let view = self
            .test
            .app
            .transactions
            .create(&self.ctx, Origin::Api, group(vec![split]))
            .await
            .unwrap();
        view.splits[0].id.clone()
    }

    fn split_on(&self, date: &str, amount: &str, bill: Option<BillId>) -> SplitInput {
        let mut split = split(1, &self.checking, &self.gym, amount);
        split.date = day(date);
        split.bill_id = bill;
        split
    }

    async fn period_id(&self, bill: BillId, due: &str) -> BillPeriodId {
        let periods = self.test.app.bills.periods(&self.ctx, bill).await.unwrap();
        periods
            .iter()
            .find(|p| p.due_date == due)
            .unwrap()
            .id
            .parse()
            .unwrap()
    }

    async fn history(&self, bill: BillId) -> BillHistoryView {
        self.test.app.bills.history(&self.ctx, bill).await.unwrap()
    }
}

fn statuses(view: &BillHistoryView) -> Vec<(&str, &str)> {
    view.due_dates
        .iter()
        .map(|d| (d.date.as_str(), d.status.as_str()))
        .collect()
}

#[tokio::test]
async fn history_lists_splits_linked_either_way_once_each() {
    let f = fixture().await;
    let bill = f.bill("2026-09-02", None).await;
    // Linked by `bill_id`; it also pays the October period automatically.
    let by_bill = f.pay(f.split_on("2026-10-03", "42.50", Some(bill))).await;
    // Linked only by paying the November period by hand.
    let by_pay = f.pay(f.split_on("2026-11-03", "45", None)).await;
    let november = f.period_id(bill, "2026-11-02").await;
    f.test
        .app
        .bills
        .pay(&f.ctx, Origin::Api, november, by_pay.parse().unwrap())
        .await
        .unwrap();
    // Unrelated to the bill.
    f.pay(f.split_on("2026-11-04", "10", None)).await;

    let view = f.history(bill).await;

    let payments: Vec<_> = view
        .payments
        .iter()
        .map(|p| {
            (
                p.split_id.as_str(),
                p.amount.as_str(),
                p.due_date.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        payments,
        [
            (by_pay.as_str(), "45.00", Some("2026-11-02")),
            (by_bill.as_str(), "42.50", Some("2026-10-02")),
        ]
    );
    let first = &view.payments[0];
    assert_eq!(
        (first.source.name.as_str(), first.destination.name.as_str()),
        ("Checking", "Gym")
    );
    assert_eq!(
        (view.amount_min.as_str(), view.amount_max.as_str()),
        ("39.00", "45.00")
    );
    assert_eq!(
        (view.range.from.as_str(), view.range.to.as_str()),
        ("2026-09-02", "2027-01-02")
    );
    assert_eq!(
        statuses(&view),
        [
            ("2026-09-02", "overdue"),
            ("2026-10-02", "paid"),
            ("2026-11-02", "paid"),
            ("2026-12-02", "open"),
            ("2027-01-02", "open"),
        ]
    );
    assert_eq!(
        view.due_dates[1].paid_split_id.as_deref(),
        Some(by_bill.as_str())
    );
    let s = &view.summary;
    assert_eq!(
        (
            s.count,
            s.total.as_deref(),
            s.average.as_deref(),
            s.min.as_deref(),
            s.max.as_deref(),
            s.other_currency_count
        ),
        (
            2,
            Some("87.50"),
            Some("43.75"),
            Some("42.50"),
            Some("45.00"),
            0
        )
    );
    assert!(!view.truncated);
}

#[tokio::test]
async fn other_currency_payments_are_listed_but_not_summed() {
    let f = fixture().await;
    let bill = f.bill("2026-09-02", None).await;
    let dollars = account(&f.test, &f.ctx, "Dollars", "asset", Some("USD")).await;
    let shop = account(&f.test, &f.ctx, "US gym", "expense", Some("USD")).await;
    let mut foreign = split(5, &dollars, &shop, "50");
    foreign.currency = Some("USD".into());
    foreign.bill_id = Some(bill);
    f.pay(foreign).await;
    // 0.01 + 0.02 = 0.03, average 0.015 → 0.02 (half away from zero).
    f.pay(f.split_on("2026-10-01", "0.01", Some(bill))).await;
    f.pay(f.split_on("2026-09-01", "0.02", Some(bill))).await;

    let view = f.history(bill).await;

    assert_eq!(view.payments.len(), 3);
    assert_eq!(
        (
            view.payments[0].amount.as_str(),
            view.payments[0].currency.as_str()
        ),
        ("50.00", "USD")
    );
    assert_eq!(view.summary.count, 2);
    assert_eq!(view.summary.total.as_deref(), Some("0.03"));
    assert_eq!(view.summary.average.as_deref(), Some("0.02"));
    assert_eq!(view.summary.other_currency_count, 1);
}

#[tokio::test]
async fn a_payment_before_the_first_due_date_starts_the_span() {
    let f = fixture().await;
    let bill = f.bill("2026-11-20", None).await;
    f.pay(f.split_on("2026-11-05", "40", Some(bill))).await;

    let view = f.history(bill).await;

    assert_eq!(view.range.from, "2026-11-05");
    assert_eq!(view.payments[0].due_date.as_deref(), Some("2026-11-20"));
}

#[tokio::test]
async fn due_dates_after_the_last_period_are_scheduled_and_not_stored() {
    let f = fixture().await;
    let bill = f.bill("2026-11-02", Some("2027-04-30")).await;
    let stored = count(&f.test, "bill_periods").await;

    let view = f.history(bill).await;

    assert_eq!(view.range.to, "2027-04-30");
    assert_eq!(
        statuses(&view),
        [
            ("2026-11-02", "overdue"),
            ("2026-12-02", "open"),
            ("2027-01-02", "open"),
            ("2027-02-02", "scheduled"),
            ("2027-03-02", "scheduled"),
            ("2027-04-02", "scheduled"),
        ]
    );
    assert!(view.due_dates[3].period_id.is_none());
    assert_eq!(count(&f.test, "bill_periods").await, stored);
}

#[tokio::test]
async fn a_bill_that_ended_stops_at_its_end_date() {
    let f = fixture().await;
    let bill = f.bill("2026-01-05", Some("2026-03-31")).await;

    let view = f.history(bill).await;

    assert_eq!(
        (view.range.from.as_str(), view.range.to.as_str()),
        ("2026-01-05", "2026-03-31")
    );
    assert_eq!(view.due_dates.len(), 3);
    assert!(view.payments.is_empty());
    assert_eq!(view.summary.count, 0);
    assert_eq!(view.summary.total, None);
    assert_eq!(view.summary.average, None);
}

#[tokio::test]
async fn the_span_never_reaches_more_than_two_years_ahead() {
    let f = fixture().await;
    let bill = f.bill("2026-11-02", Some("2040-01-01")).await;

    let view = f.history(bill).await;

    assert_eq!(view.range.to, "2028-11-13");
    assert_eq!(view.due_dates.last().unwrap().date, "2028-11-02");
}

#[tokio::test]
async fn another_households_bill_is_not_found() {
    let f = fixture().await;
    let other = other_household(&f.test).await;
    let foreign: BillId = f
        .test
        .app
        .bills
        .create(&other, Origin::Api, gym("2026-09-02", None))
        .await
        .unwrap()
        .id
        .parse()
        .unwrap();

    let result = f.test.app.bills.history(&f.ctx, foreign).await;

    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn only_the_latest_due_dates_are_kept() {
    let f = fixture().await;
    let mut weekly = gym("1985-01-01", None);
    weekly.repeat_freq = "weekly".into();
    let bill: BillId = f
        .test
        .app
        .bills
        .create(&f.ctx, Origin::Api, weekly)
        .await
        .unwrap()
        .id
        .parse()
        .unwrap();

    let view = f.history(bill).await;

    assert!(view.truncated);
    assert_eq!(view.due_dates.len(), saldo_app::bills::MAX_DUE_DATES);
    assert_eq!(view.range.from, "1985-01-01");
    assert!(view.due_dates[0].date.as_str() > "1985-01-01");
}
