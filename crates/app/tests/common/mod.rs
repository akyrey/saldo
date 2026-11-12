//! Shared helpers for the integration tests: an app on a fresh in-memory
//! database with the real migrations, a controllable clock, and a household.

#![allow(dead_code)] // each test file uses a different subset
#![allow(clippy::unwrap_used)] // test-only helpers; clippy does not see them as tests

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use saldo_app::clock::FixedClock;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::events::{EventFilter, EventView};
use saldo_app::identity::{NewMember, SetupInput, SetupResult};
use saldo_app::page::PageRequest;
use saldo_app::{App, db};

pub const PASSWORD: &str = "correct horse battery";

pub struct TestApp {
    pub app: App,
    pub clock: Arc<FixedClock>,
}

pub async fn test_app() -> TestApp {
    let pool = db::connect_in_memory().await.unwrap();
    db::migrate(&pool).await.unwrap();
    let clock = Arc::new(FixedClock::new(
        Utc.with_ymd_and_hms(2026, 11, 13, 9, 0, 0).unwrap(),
    ));
    TestApp {
        app: App::new(pool, clock.clone()),
        clock,
    }
}

pub fn setup_input(email: &str) -> SetupInput {
    SetupInput {
        email: email.to_string(),
        password: PASSWORD.to_string(),
        display_name: "Owner".to_string(),
        household_name: "Home".to_string(),
        primary_currency: "EUR".to_string(),
    }
}

/// An app with a household whose owner is `owner@example.com`.
pub async fn with_household() -> (TestApp, SetupResult) {
    let test = test_app().await;
    let setup = test
        .app
        .identity
        .setup(Origin::Api, setup_input("owner@example.com"))
        .await
        .unwrap();
    (test, setup)
}

pub async fn add_member(test: &TestApp, owner: &HouseholdCtx, email: &str) -> HouseholdCtx {
    let input = NewMember {
        email: email.to_string(),
        display_name: "Member".to_string(),
        password: PASSWORD.to_string(),
    };
    test.app
        .identity
        .add_member(owner, Origin::Api, input)
        .await
        .unwrap();
    test.app.identity.login(email, PASSWORD).await.unwrap().ctx
}

/// Every event of the household, oldest first.
pub async fn events(test: &TestApp, ctx: &HouseholdCtx) -> Vec<EventView> {
    let page = PageRequest::new(None, Some(200)).unwrap();
    test.app
        .events
        .list(ctx, &EventFilter::default(), page)
        .await
        .unwrap()
        .items
}

pub async fn last_event(test: &TestApp, ctx: &HouseholdCtx) -> EventView {
    events(test, ctx).await.pop().unwrap()
}

pub async fn count(test: &TestApp, table: &str) -> i64 {
    let sql = format!("SELECT count(*) FROM {table}");
    let (n,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .fetch_one(&test.app.pool)
        .await
        .unwrap();
    n
}
