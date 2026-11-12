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

use chrono::NaiveDate;
use saldo_app::accounts::{AccountView, NewAccount};
use saldo_app::transactions::{SplitInput, TransactionInput};
use saldo_core::{AccountId, HouseholdId, UserId};
use uuid::Uuid;

/// A second, unrelated household, inserted directly (setup only works once).
pub async fn other_household(test: &TestApp) -> HouseholdCtx {
    let ctx = HouseholdCtx {
        household_id: HouseholdId(Uuid::from_u128(0xB0B)),
        user_id: UserId(Uuid::from_u128(0xB0B1)),
        role: saldo_app::ctx::Role::Owner,
    };
    let (h, u) = (ctx.household_id.to_string(), ctx.user_id.to_string());
    for sql in [
        "INSERT INTO households (id, name, primary_currency, created_at, updated_at) VALUES (?1, 'Other', 'EUR', 't', 't')",
        "INSERT INTO users (id, email, display_name, password_hash, created_at, updated_at) VALUES (?2, 'b@example.com', 'B', 'x', 't', 't')",
        "INSERT INTO household_members (household_id, user_id, role, created_at) VALUES (?1, ?2, 'owner', 't')",
    ] {
        sqlx::query(sql)
            .bind(&h)
            .bind(&u)
            .execute(&test.app.pool)
            .await
            .unwrap();
    }
    ctx
}

pub async fn account(
    test: &TestApp,
    ctx: &HouseholdCtx,
    name: &str,
    account_type: &str,
    currency: Option<&str>,
) -> AccountView {
    let input = NewAccount {
        name: name.to_string(),
        account_type: account_type.to_string(),
        currency: currency.map(str::to_string),
        ..NewAccount::default()
    };
    test.app
        .accounts
        .create(ctx, Origin::Api, input)
        .await
        .unwrap()
}

pub fn account_id(view: &AccountView) -> AccountId {
    view.id.parse().unwrap()
}

pub fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, day).unwrap()
}

pub fn split(
    day: u32,
    source: &AccountView,
    destination: &AccountView,
    amount: &str,
) -> SplitInput {
    SplitInput {
        date: date(day),
        description: format!("{} to {}", source.name, destination.name),
        source_id: account_id(source),
        destination_id: account_id(destination),
        amount: amount.to_string(),
        currency: None,
        foreign_amount: None,
        foreign_currency: None,
        category_id: None,
        budget_id: None,
        bill_id: None,
        tag_ids: Vec::new(),
        reconciled: false,
        notes: None,
        meta: serde_json::Map::new(),
    }
}

pub fn group(splits: Vec<SplitInput>) -> TransactionInput {
    TransactionInput {
        title: Some("Test".to_string()),
        splits,
    }
}
