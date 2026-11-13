#![allow(clippy::unwrap_used, clippy::expect_used)] // test helpers

//! Piggy banks (from a fixture written to the API specification) and
//! recurrences (from the owner-shaped fixture), imported twice.

use std::sync::Arc;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use saldo_app::clock::FixedClock;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::identity::SetupInput;
use saldo_app::page::PageRequest;
use saldo_app::{App, db};
use saldo_importer::engine::{Options, run};
use saldo_importer::firefly::FireflyClient;
use saldo_importer::report::{Counts, ImportReport};
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture(name: &str) -> Value {
    let file = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
}

fn empty_page() -> Value {
    json!({ "data": [], "meta": { "pagination": { "total": 0, "count": 0, "per_page": 500, "current_page": 1, "total_pages": 1 } } })
}

async fn mount(server: &MockServer, endpoint: &str, body: Value) {
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/{endpoint}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

async fn firefly() -> MockServer {
    firefly_with(
        fixture("piggy-banks-spec.json"),
        fixture("recurrences.json"),
    )
    .await
}

/// A fake Firefly with the given piggy banks and recurrences.
async fn firefly_with(piggy_banks: Value, recurrences: Value) -> MockServer {
    let server = MockServer::start().await;
    for (endpoint, file) in [
        ("currencies", "currencies.json"),
        ("accounts", "accounts.json"),
        ("categories", "categories.json"),
        ("tags", "tags.json"),
        ("attachments", "attachments.json"),
        ("piggy-banks/1/events", "piggy-bank-events-1.json"),
    ] {
        mount(&server, endpoint, fixture(file)).await;
    }
    mount(&server, "piggy-banks", piggy_banks).await;
    mount(&server, "recurrences", recurrences).await;
    mount(&server, "piggy-banks/2/events", empty_page()).await;
    // Entities imported by the other Phase 5 tracks, empty here.
    for endpoint in ["budgets", "subscriptions", "rule-groups", "rules"] {
        mount(&server, endpoint, empty_page()).await;
    }
    for page in ["1", "2"] {
        Mock::given(method("GET"))
            .and(path("/api/v1/transactions"))
            .and(query_param("page", page))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(fixture(&format!("transactions-{page}.json"))),
            )
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v1/attachments/1/download"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"receipt".to_vec()))
        .mount(&server)
        .await;
    server
}

async fn household() -> (App, HouseholdCtx) {
    let pool = db::connect_in_memory().await.unwrap();
    db::migrate(&pool).await.unwrap();
    let clock = Arc::new(FixedClock::new(
        Utc.with_ymd_and_hms(2026, 11, 13, 9, 0, 0).unwrap(),
    ));
    let data_dir = std::env::temp_dir().join(format!("saldo-import-test-{}", uuid::Uuid::new_v4()));
    let app = App::new(pool, clock).with_data_dir(data_dir);
    let input = SetupInput {
        email: "owner@example.com".into(),
        password: "correct horse battery".into(),
        display_name: "Owner".into(),
        household_name: "Home".into(),
        primary_currency: "EUR".into(),
    };
    let ctx = app.identity.setup(Origin::Api, input).await.unwrap().ctx;
    (app, ctx)
}

async fn import(app: &App, ctx: HouseholdCtx, server: &MockServer) -> ImportReport {
    let client = FireflyClient::new(&server.uri(), "secret-token".into())
        .unwrap()
        .with_backoff(Duration::from_millis(1));
    run(app.clone(), client, ctx, Options::default())
        .await
        .unwrap()
}

fn counts(report: &ImportReport, entity: &str) -> Counts {
    report.entities.get(entity).copied().unwrap_or_default()
}

/// Every row of the tables these importers write, as JSON text.
async fn snapshot(app: &App) -> Vec<String> {
    let mut rows = Vec::new();
    for table in [
        "piggy_banks",
        "piggy_bank_accounts",
        "piggy_bank_events",
        "recurrences",
        "recurrence_repetitions",
        "recurrence_splits",
        "external_refs",
    ] {
        let columns: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT name FROM pragma_table_info('{table}')"
        )))
        .fetch_all(&app.pool)
        .await
        .unwrap();
        let fields = columns
            .iter()
            .map(|(c,)| format!("'{c}', {c}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("SELECT '{table}' || json_object({fields}) FROM {table} ORDER BY 1");
        let table_rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .fetch_all(&app.pool)
            .await
            .unwrap();
        rows.extend(table_rows.into_iter().map(|(r,)| r));
    }
    rows
}

#[tokio::test]
async fn piggy_banks_and_their_events_are_imported_once() {
    let (app, ctx) = household().await;
    let server = firefly().await;
    let report = import(&app, ctx, &server).await;

    assert_eq!(
        counts(&report, "piggy_banks"),
        Counts {
            created: 2,
            updated: 0,
            unchanged: 0,
            skipped: 1
        }
    );
    assert!(
        report
            .messages
            .iter()
            .any(|m| m.contains("Lost account") && m.contains("99"))
    );
    assert_eq!(counts(&report, "piggy_bank_events").created, 3);

    let page = app
        .piggy_banks
        .list(&ctx, None, PageRequest::default())
        .await
        .unwrap();
    let holiday = &page.items[0];
    assert_eq!(holiday.name, "Holiday");
    assert_eq!(holiday.target_amount.as_deref(), Some("300.00"));
    assert_eq!(holiday.saved_amount, "115.00");
    assert_eq!(holiday.start_date.as_deref(), Some("2026-10-01"));
    assert_eq!(holiday.notes.as_deref(), Some("Summer trip"));
    assert_eq!(page.items[1].target_amount, None, "\"0\" means no target");

    let events = app
        .piggy_banks
        .list_events(&ctx, holiday.id.parse().unwrap(), PageRequest::default())
        .await
        .unwrap();
    let linked: Vec<_> = events
        .items
        .iter()
        .filter(|e| e.split_id.is_some())
        .collect();
    assert_eq!(linked.len(), 1);
    assert_eq!(linked[0].amount, "100.00");
    assert_eq!(linked[0].date, "2026-11-05");
}

#[tokio::test]
async fn recurrences_map_firefly_repetitions_and_never_backfill() {
    let (app, ctx) = household().await;
    let server = firefly().await;
    let report = import(&app, ctx, &server).await;

    assert_eq!(counts(&report, "recurrences").created, 3);
    assert_eq!(
        report.notes["recurrence: subscription not imported, link dropped"],
        1
    );
    let page = app
        .recurrences
        .list(&ctx, None, PageRequest::default())
        .await
        .unwrap();
    let by_title = |title: &str| page.items.iter().find(|r| r.title == title).unwrap();

    let rent = by_title("Monthly rent");
    assert_eq!(rent.repetitions[0].kind, "monthly");
    assert_eq!(rent.repetitions[0].moment, "2");
    assert_eq!(rent.splits[0].amount, "800.00");
    assert!(rent.splits[0].category_id.is_some());
    assert_eq!(rent.splits[0].bill_id, None);
    assert_eq!(rent.fired_through.as_deref(), Some("2026-11-13"));

    let insurance = by_title("Insurance");
    assert_eq!(insurance.repetitions[0].kind, "yearly");
    assert_eq!(insurance.repetitions[0].moment, "03-15");
    assert_eq!(insurance.repetitions[0].weekend_policy, "next_monday");
    assert_eq!(insurance.max_occurrences, Some(4));
    assert_eq!(insurance.repeat_until.as_deref(), Some("2030-12-31"));
    assert!(!insurance.apply_rules);
    assert_eq!(insurance.splits[0].tag_ids.len(), 1);

    let club = by_title("Club dues");
    assert_eq!(club.repetitions[0].kind, "nth_weekday");
    assert_eq!(club.repetitions[0].moment, "2,3");
    assert_eq!(club.repetitions[0].skip, 1);
    assert_eq!(club.repetitions[0].weekend_policy, "previous_friday");
    assert!(!club.active);

    // Firefly created the past transactions already: nothing is due.
    let due = app.recurrences.fire_due(&ctx).await.unwrap();
    assert!(due.fired.is_empty() && due.failures.is_empty());
}

#[tokio::test]
async fn a_template_in_a_foreign_currency_is_booked_in_the_account_currency() {
    // Firefly allows a template whose amount is in another currency than its
    // asset account, with the account's currency as the foreign amount.
    let mut recurrences = fixture("recurrences.json");
    let mut sponsor = recurrences["data"][0].clone();
    sponsor["id"] = "99".into();
    sponsor["attributes"]["title"] = "Sponsor".into();
    let split = &mut sponsor["attributes"]["transactions"][0];
    split["currency_code"] = "USD".into();
    split["amount"] = "1.000000000000".into();
    split["foreign_currency_code"] = "EUR".into();
    split["foreign_amount"] = "0.940000000000".into();
    split["destination_id"] = "5".into();
    recurrences["data"].as_array_mut().unwrap().push(sponsor);
    let (app, ctx) = household().await;
    let server = firefly_with(fixture("piggy-banks-spec.json"), recurrences).await;

    let report = import(&app, ctx, &server).await;

    assert_eq!(counts(&report, "recurrences").created, 4);
    assert_eq!(
        report.notes["recurrence: amount and foreign amount swapped to the account's currency"],
        1
    );
    let page = app
        .recurrences
        .list(&ctx, None, PageRequest::default())
        .await
        .unwrap();
    let sponsor = page.items.iter().find(|r| r.title == "Sponsor").unwrap();
    let split = &sponsor.splits[0];
    assert_eq!(
        (split.amount.as_str(), split.currency.as_str()),
        ("0.94", "EUR")
    );
    assert_eq!(split.foreign_amount.as_deref(), Some("1.00"));
    assert_eq!(split.foreign_currency.as_deref(), Some("USD"));
    // A second run sees the same, swapped template: nothing changes.
    let again = import(&app, ctx, &server).await;
    assert_eq!(counts(&again, "recurrences").unchanged, 4);
}

#[tokio::test]
async fn a_second_import_changes_nothing() {
    let (app, ctx) = household().await;
    let server = firefly().await;
    import(&app, ctx, &server).await;
    let before = snapshot(&app).await;

    let second = import(&app, ctx, &server).await;

    assert!(!second.has_changes(), "{}", second.render());
    assert_eq!(counts(&second, "piggy_banks").unchanged, 2);
    assert_eq!(counts(&second, "piggy_bank_events").unchanged, 3);
    assert_eq!(counts(&second, "recurrences").unchanged, 3);
    assert_eq!(snapshot(&app).await, before, "the tables must be identical");
}

#[tokio::test]
async fn changed_piggy_banks_and_recurrences_are_updated() {
    let (app, ctx) = household().await;
    import(&app, ctx, &firefly().await).await;

    let mut banks = fixture("piggy-banks-spec.json");
    banks["data"][0]["attributes"]["name"] = "Holiday 2027".into();
    let mut recurrences = fixture("recurrences.json");
    recurrences["data"][0]["attributes"]["transactions"][0]["amount"] = "850.000000000000".into();
    let server = firefly_with(banks, recurrences).await;

    let report = import(&app, ctx, &server).await;

    assert_eq!(counts(&report, "piggy_banks").updated, 1);
    assert_eq!(counts(&report, "recurrences").updated, 1);
    let page = app
        .recurrences
        .list(&ctx, None, PageRequest::default())
        .await
        .unwrap();
    let rent = page
        .items
        .iter()
        .find(|r| r.title == "Monthly rent")
        .unwrap();
    assert_eq!(rent.splits[0].amount, "850.00");
    assert_eq!(
        rent.fired_through.as_deref(),
        Some("2026-11-13"),
        "kept on update"
    );
}
