//! The bill scheduler over a simulated month (ARCHITECTURE.md 8.3).

#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use chrono::{NaiveDate, TimeDelta};
use common::*;
use saldo_app::bills::{NewBill, run_reminders};
use saldo_app::clock::Clock;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::identity::HouseholdUpdate;
use saldo_core::BillId;

fn bill(first_due: &str, remind_every: u32) -> NewBill {
    NewBill {
        name: "Internet".into(),
        amount_min: "29.90".into(),
        amount_max: "35".into(),
        currency: None,
        first_due_date: first_due.parse().unwrap(),
        end_date: None,
        repeat_freq: "monthly".into(),
        skip: None,
        remind_days_before: Some(3),
        remind_every_days: Some(remind_every),
        active: None,
        sort_order: None,
        notes: None,
    }
}

/// `(household date, event type)` of every reminder event so far.
async fn reminders(test: &TestApp, ctx: &HouseholdCtx) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for event in events(test, ctx).await {
        if event.event_type == "bill.due_soon" || event.event_type == "bill.overdue" {
            let day = event.payload["data"]["period"]["last_reminded_on"]
                .as_str()
                .unwrap()
                .to_string();
            found.push((day, event.event_type));
        }
    }
    found
}

async fn run(test: &TestApp) -> u32 {
    run_reminders(&test.app.identity, &test.app.bills)
        .await
        .unwrap()
}

/// Runs the scheduler four times a day (every 6 hours) until `until`.
async fn simulate_until(test: &TestApp, until: &str) {
    let until: NaiveDate = until.parse().unwrap();
    while test.clock.now().date_naive() < until {
        run(test).await;
        test.clock.advance(TimeDelta::hours(6));
    }
}

fn expected(rows: &[(&str, &str)]) -> Vec<(String, String)> {
    rows.iter()
        .map(|(d, t)| (d.to_string(), t.to_string()))
        .collect()
}

#[tokio::test]
async fn a_month_gives_one_due_soon_then_overdue_every_n_days_until_paid() {
    let (test, setup) = with_household().await;
    // Today is 2026-11-13; due on the 20th, reminders 3 days before, then
    // every 2 days once overdue.
    let view = test
        .app
        .bills
        .create(&setup.ctx, Origin::Api, bill("2026-11-20", 2))
        .await
        .unwrap();
    simulate_until(&test, "2026-11-26").await;
    assert_eq!(
        reminders(&test, &setup.ctx).await,
        expected(&[
            ("2026-11-17", "bill.due_soon"),
            ("2026-11-21", "bill.overdue"),
            ("2026-11-23", "bill.overdue"),
            ("2026-11-25", "bill.overdue"),
        ])
    );

    // Paid on the 26th: no reminder for this period any more.
    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let isp = account(&test, &setup.ctx, "ISP", "expense", None).await;
    let mut payment = split(26, &checking, &isp, "32.50");
    payment.bill_id = Some(view.id.parse::<BillId>().unwrap());
    test.app
        .transactions
        .create(&setup.ctx, Origin::Api, group(vec![payment]))
        .await
        .unwrap();
    // Up to the day before December's due-soon reminder (due 12-20, remind 12-17).
    simulate_until(&test, "2026-12-17").await;
    assert_eq!(reminders(&test, &setup.ctx).await.len(), 4);

    simulate_until(&test, "2026-12-18").await;
    assert_eq!(
        reminders(&test, &setup.ctx).await.last().unwrap(),
        &("2026-12-17".to_string(), "bill.due_soon".to_string())
    );
}

#[tokio::test]
async fn running_again_on_the_same_day_emits_nothing_new() {
    let (test, setup) = with_household().await;
    test.app
        .bills
        .create(&setup.ctx, Origin::Api, bill("2026-11-14", 1))
        .await
        .unwrap();
    assert_eq!(run(&test).await, 1);
    let before = events(&test, &setup.ctx).await.len();
    // Two "restarts" later the same day.
    test.clock.advance(TimeDelta::hours(2));
    assert_eq!(run(&test).await, 0);
    assert_eq!(run(&test).await, 0);
    assert_eq!(events(&test, &setup.ctx).await.len(), before);
}

#[tokio::test]
async fn the_household_timezone_decides_today() {
    let (test, setup) = with_household().await;
    test.app
        .bills
        .create(&setup.ctx, Origin::Api, bill("2026-11-20", 1))
        .await
        .unwrap();
    // 2026-11-16 12:00 UTC: still the 16th in UTC, already the 17th in Auckland.
    test.clock.advance(TimeDelta::hours(3 * 24 + 3));
    assert_eq!(run(&test).await, 0);
    let update = HouseholdUpdate {
        timezone: Some("Pacific/Auckland".into()),
        ..Default::default()
    };
    test.app
        .identity
        .update_household(&setup.ctx, Origin::Api, update)
        .await
        .unwrap();
    assert_eq!(run(&test).await, 1);
    assert_eq!(
        reminders(&test, &setup.ctx).await,
        expected(&[("2026-11-17", "bill.due_soon")])
    );
}

#[tokio::test]
async fn every_household_is_reminded_and_events_come_from_the_scheduler() {
    let (test, setup) = with_household().await;
    let other = other_household(&test).await;
    for ctx in [&setup.ctx, &other] {
        test.app
            .bills
            .create(ctx, Origin::Api, bill("2026-11-14", 1))
            .await
            .unwrap();
    }
    assert_eq!(run(&test).await, 2);
    let event = last_event(&test, &other).await;
    assert_eq!(event.event_type, "bill.due_soon");
    assert_eq!(event.origin, "scheduler");
    assert_eq!(event.payload["data"]["bill"]["name"], "Internet");
}
