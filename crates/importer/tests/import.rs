#![allow(clippy::unwrap_used, clippy::expect_used)] // test helpers

use std::sync::Arc;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use saldo_app::clock::FixedClock;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::identity::SetupInput;
use saldo_app::{App, db};
use saldo_importer::engine::{Options, run};
use saldo_importer::firefly::FireflyClient;
use saldo_importer::report::{Counts, ImportReport};
use serde_json::Value;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture(name: &str) -> Value {
    let text = std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

/// A fake Firefly serving the fixtures; the two transaction pages can be replaced.
async fn firefly(page_one: Option<Value>, page_two: Option<Value>) -> MockServer {
    let server = MockServer::start().await;
    for (endpoint, file) in [
        ("currencies", "currencies.json"),
        ("accounts", "accounts.json"),
        ("categories", "categories.json"),
        ("tags", "tags.json"),
        ("attachments", "attachments.json"),
        ("subscriptions", "subscriptions.json"),
        ("rule-groups", "rule-groups.json"),
        ("rules", "rules.json"),
        ("piggy-banks", "piggy-banks.json"),
        ("recurrences", "recurrences.json"),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v1/{endpoint}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture(file)))
            .mount(&server)
            .await;
    }
    let pages = [
        page_one.unwrap_or_else(|| fixture("transactions-1.json")),
        page_two.unwrap_or_else(|| fixture("transactions-2.json")),
    ];
    for (number, body) in ["1", "2"].iter().zip(pages) {
        Mock::given(method("GET"))
            .and(path("/api/v1/transactions"))
            .and(query_param("page", *number))
            .and(query_param("type", "all"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v1/attachments/1/download"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"receipt: 42.50 EUR".to_vec()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/attachments/2/download"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"%PDF".to_vec()))
        .mount(&server)
        .await;
    mount_budgets(&server, fixture("budgets.json")).await;
    server
}

/// `/v1/budgets` with `budgets`, and the limit fixtures of budgets 1 and 2.
async fn mount_budgets(server: &MockServer, budgets: Value) {
    Mock::given(method("GET"))
        .and(path("/api/v1/budgets"))
        .respond_with(ResponseTemplate::new(200).set_body_json(budgets))
        .mount(server)
        .await;
    for id in ["1", "2"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v1/budgets/{id}/limits")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(fixture(&format!("budget-limits-{id}.json"))),
            )
            .mount(server)
            .await;
    }
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

async fn import(app: &App, ctx: HouseholdCtx, server: &MockServer, dry_run: bool) -> ImportReport {
    let client = FireflyClient::new(&server.uri(), "secret-token".into())
        .unwrap()
        .with_backoff(Duration::from_millis(1));
    run(
        app.clone(),
        client,
        ctx,
        Options {
            dry_run,
            ..Options::default()
        },
    )
    .await
    .unwrap()
}

fn counts(report: &ImportReport, entity: &str) -> Counts {
    report.entities.get(entity).copied().unwrap_or_default()
}

/// Every row of the domain tables as JSON text, for comparing whole databases.
async fn snapshot(app: &App) -> Vec<String> {
    let tables = [
        "currencies",
        "accounts",
        "categories",
        "tags",
        "budgets",
        "budget_limits",
        "transaction_groups",
        "splits",
        "split_tags",
        "attachments",
        "external_refs",
        "bills",
        "bill_periods",
        "rule_groups",
        "rules",
        "rule_triggers",
        "rule_actions",
    ];
    let mut rows = Vec::new();
    for table in tables {
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

async fn scalar(app: &App, sql: &str) -> i64 {
    let (n,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(sql.to_string()))
        .fetch_one(&app.pool)
        .await
        .unwrap();
    n
}

#[tokio::test]
async fn first_import_creates_everything_and_balances_match() {
    let (app, ctx) = household().await;
    let server = firefly(None, None).await;
    let report = import(&app, ctx, &server, false).await;

    assert_eq!(
        counts(&report, "currencies"),
        Counts {
            created: 1,
            updated: 0,
            unchanged: 2,
            skipped: 0
        }
    );
    // 7 regular accounts + the two (cash) accounts; 3 mapped to equity; 1 import account skipped.
    assert_eq!(
        counts(&report, "accounts"),
        Counts {
            created: 9,
            updated: 0,
            unchanged: 3,
            skipped: 1
        }
    );
    assert_eq!(counts(&report, "categories").created, 3);
    assert_eq!(counts(&report, "tags").created, 4);
    assert_eq!(
        counts(&report, "transactions"),
        Counts {
            created: 12,
            updated: 0,
            unchanged: 0,
            skipped: 0
        }
    );
    assert_eq!(
        report.notes["foreign amount in the split's own currency dropped"],
        1
    );
    assert_eq!(
        report.notes["kind: Firefly withdrawal is Saldo transfer"],
        1
    );
    assert_eq!(report.balances.len(), 4);
    assert_eq!(report.balance_mismatches(), 0, "{}", report.render());

    assert_eq!(scalar(&app, "SELECT count(*) FROM splits").await, 13);
    assert_eq!(scalar(&app, "SELECT count(*) FROM split_tags").await, 4);
    assert_eq!(
        scalar(
            &app,
            "SELECT count(*) FROM splits WHERE kind = 'opening_balance'"
        )
        .await,
        2
    );
    assert_eq!(scalar(&app, "SELECT count(*) FROM events WHERE origin = 'import' AND event_type = 'transaction.created'").await, 12);
    assert_eq!(
        scalar(
            &app,
            "SELECT count(*) FROM import_runs WHERE status = 'succeeded'"
        )
        .await,
        1
    );
    let meta: (String,) =
        sqlx::query_as("SELECT meta FROM splits WHERE description = 'Rent November'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert!(meta.0.contains("\"sepa_ct_id\":\"REF-001\"") && meta.0.contains("\"journal_id\""));
    let summary: (String,) = sqlx::query_as("SELECT summary FROM import_runs")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert!(
        !summary.0.contains("secret-token"),
        "the token must never be stored"
    );
    assert_eq!(
        counts(&report, "attachments").created,
        2,
        "one on a split, one on a bill"
    );
    assert_eq!(
        scalar(
            &app,
            "SELECT count(*) FROM attachments WHERE attachable_type = 'bill'"
        )
        .await,
        1
    );
    assert_eq!(counts(&report, "bills").created, 2);
    assert_eq!(counts(&report, "rule_groups").created, 1);
    assert_eq!(counts(&report, "rules").created, 3);
    let inactive: Vec<(String,)> = sqlx::query_as("SELECT title FROM rules WHERE active = 0")
        .fetch_all(&app.pool)
        .await
        .unwrap();
    assert_eq!(
        inactive,
        [("Exotic".to_string(),)],
        "unsupported journal_id trigger"
    );
    assert!(
        report
            .messages
            .iter()
            .any(|m| m.contains("\"Exotic\" imported inactive")),
        "{:?}",
        report.messages
    );
    let (kind, value): (String, Option<String>) = sqlx::query_as(
        "SELECT a.kind, a.value FROM rule_actions a JOIN rules r ON r.id = a.rule_id WHERE r.title = 'Rent' AND a.kind = 'link_to_bill'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    let rent_bill: (String,) = sqlx::query_as("SELECT id FROM bills WHERE name = 'Rent'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(
        (kind, value),
        ("link_to_bill".to_string(), Some(rent_bill.0))
    );
    let currency: (Option<String>,) =
        sqlx::query_as("SELECT value FROM rule_triggers WHERE kind = 'currency_is'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(currency.0.as_deref(), Some("EUR"));
    // The two rent splits linked to the subscription paid November and January.
    let paid: Vec<(String,)> =
        sqlx::query_as("SELECT due_date FROM bill_periods WHERE status = 'paid' ORDER BY due_date")
            .fetch_all(&app.pool)
            .await
            .unwrap();
    assert_eq!(
        paid,
        [("2026-11-02".to_string(),), ("2027-01-02".to_string(),)]
    );
}

#[tokio::test]
async fn second_import_changes_nothing() {
    let (app, ctx) = household().await;
    let server = firefly(None, None).await;
    import(&app, ctx, &server, false).await;
    let before = snapshot(&app).await;
    let events_before = scalar(
        &app,
        "SELECT count(*) FROM events WHERE event_type <> 'import.completed'",
    )
    .await;

    let second = import(&app, ctx, &server, false).await;

    assert!(!second.has_changes(), "{}", second.render());
    assert_eq!(counts(&second, "transactions").unchanged, 12);
    assert_eq!(counts(&second, "accounts").unchanged, 12);
    assert_eq!(counts(&second, "attachments").unchanged, 2);
    assert_eq!(counts(&second, "bills").unchanged, 2);
    assert_eq!(counts(&second, "rules").unchanged, 3);
    assert_eq!(counts(&second, "rule_groups").unchanged, 1);
    assert_eq!(second.balance_mismatches(), 0);
    assert_eq!(
        snapshot(&app).await,
        before,
        "the database must be identical"
    );
    assert_eq!(
        scalar(
            &app,
            "SELECT count(*) FROM events WHERE event_type <> 'import.completed'"
        )
        .await,
        events_before
    );
}

#[tokio::test]
async fn dry_run_reports_but_saves_nothing() {
    let (app, ctx) = household().await;
    let server = firefly(None, None).await;
    let report = import(&app, ctx, &server, true).await;
    assert!(report.dry_run);
    assert_eq!(counts(&report, "transactions").created, 12);
    assert_eq!(report.balance_mismatches(), 0);
    assert_eq!(scalar(&app, "SELECT count(*) FROM splits").await, 0);
    assert_eq!(scalar(&app, "SELECT count(*) FROM external_refs").await, 0);
    assert_eq!(scalar(&app, "SELECT count(*) FROM attachments").await, 0);
    assert_eq!(counts(&report, "attachments").created, 2);
    assert_eq!(scalar(&app, "SELECT count(*) FROM bills").await, 0);
    assert_eq!(scalar(&app, "SELECT count(*) FROM import_runs").await, 1);
}

#[tokio::test]
async fn changed_groups_are_replaced_and_removed_splits_forgotten() {
    let (app, ctx) = household().await;
    import(&app, ctx, &firefly(None, None).await, false).await;

    // In Firefly: the rent split of group 4 is deleted and the coffee costs more.
    // Both live on page 2 (group 4 straddles the pages: rent is its second part).
    let mut page_one = fixture("transactions-1.json");
    let mut page_two = fixture("transactions-2.json");
    let groups = page_two["data"].as_array_mut().unwrap();
    let rent = groups.remove(0);
    assert_eq!(rent["id"], "4");
    let rent_journal = rent["attributes"]["transactions"][0]["transaction_journal_id"]
        .as_str()
        .unwrap()
        .to_string();
    let coffee = groups.iter_mut().find(|g| g["id"] == "6").unwrap();
    coffee["attributes"]["transactions"][0]["amount"] = "5.500000000000".into();
    page_one["meta"]["pagination"]["total"] = 12.into();
    page_two["meta"]["pagination"]["total"] = 12.into();
    let server = firefly(Some(page_one), Some(page_two)).await;

    let report = import(&app, ctx, &server, false).await;

    assert_eq!(
        counts(&report, "transactions"),
        Counts {
            created: 0,
            updated: 2,
            unchanged: 10,
            skipped: 0
        }
    );
    assert_eq!(scalar(&app, "SELECT count(*) FROM splits").await, 12);
    let rent_refs = scalar(&app, &format!("SELECT count(*) FROM external_refs WHERE entity_type = 'split' AND external_id = '{rent_journal}'")).await;
    assert_eq!(rent_refs, 0);
    assert_eq!(
        scalar(
            &app,
            "SELECT count(*) FROM external_refs WHERE entity_type = 'split'"
        )
        .await,
        12
    );
}

#[tokio::test]
async fn a_changed_group_keeps_its_splits_and_their_attachments() {
    let (app, ctx) = household().await;
    import(&app, ctx, &firefly(None, None).await, false).await;
    let split_of_receipt = "SELECT attachable_id FROM attachments WHERE filename = 'receipt.txt'";
    let before: String = sqlx::query_scalar(split_of_receipt)
        .fetch_one(&app.pool)
        .await
        .unwrap();

    // In Firefly the split with the receipt (journal 104) now costs more.
    let mut page_one = fixture("transactions-1.json");
    let journal = page_one["data"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .flat_map(|g| {
            g["attributes"]["transactions"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
        })
        .find(|s| s["transaction_journal_id"] == "104")
        .unwrap();
    journal["amount"] = "99.000000000000".into();
    let report = import(&app, ctx, &firefly(Some(page_one), None).await, false).await;

    assert_eq!(counts(&report, "transactions").updated, 1);
    let after: String = sqlx::query_scalar(split_of_receipt)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(
        after, before,
        "the attachment still points at the same split"
    );
    let amount = scalar(
        &app,
        &format!("SELECT amount_minor FROM splits WHERE id = '{after}'"),
    )
    .await;
    assert_eq!(amount, 9900);
}

#[tokio::test]
async fn an_attachment_firefly_cannot_serve_is_skipped() {
    let (app, ctx) = household().await;
    let server = MockServer::start().await;
    // Mounted first, so it wins over the working download in `firefly()`.
    Mock::given(method("GET"))
        .and(path("/api/v1/attachments/1/download"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/attachments/2/download"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"%PDF".to_vec()))
        .mount(&server)
        .await;
    for request in [
        ("currencies", "currencies.json"),
        ("accounts", "accounts.json"),
        ("categories", "categories.json"),
        ("tags", "tags.json"),
        ("attachments", "attachments.json"),
        ("subscriptions", "subscriptions.json"),
        ("rule-groups", "rule-groups.json"),
        ("rules", "rules.json"),
        ("piggy-banks", "piggy-banks.json"),
        ("recurrences", "recurrences.json"),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v1/{}", request.0)))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture(request.1)))
            .mount(&server)
            .await;
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
    mount_budgets(&server, fixture("budgets.json")).await;

    let report = import(&app, ctx, &server, false).await;

    assert_eq!(
        counts(&report, "attachments"),
        Counts {
            created: 1,
            updated: 0,
            unchanged: 0,
            skipped: 1
        }
    );
    assert!(report.messages.iter().any(|m| m.contains("HTTP 500")));
    assert_eq!(
        counts(&report, "transactions").created,
        12,
        "the rest of the import still happened"
    );
}

/// A fake Firefly whose accounts, categories and tags can be replaced.
async fn firefly_with(accounts: Value, categories: Value, tags: Value) -> MockServer {
    let server = MockServer::start().await;
    for (endpoint, body) in [
        ("accounts", accounts),
        ("categories", categories),
        ("tags", tags),
        ("currencies", fixture("currencies.json")),
        ("attachments", fixture("attachments.json")),
        ("subscriptions", fixture("subscriptions.json")),
        ("rule-groups", fixture("rule-groups.json")),
        ("rules", fixture("rules.json")),
        ("piggy-banks", fixture("piggy-banks.json")),
        ("recurrences", fixture("recurrences.json")),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v1/{endpoint}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
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
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"receipt: 42.50 EUR".to_vec()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/attachments/2/download"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"%PDF".to_vec()))
        .mount(&server)
        .await;
    mount_budgets(&server, fixture("budgets.json")).await;
    server
}

#[tokio::test]
async fn renamed_objects_are_updated_and_bad_ones_skipped() {
    let (app, ctx) = household().await;
    import(&app, ctx, &firefly(None, None).await, false).await;

    let mut accounts = fixture("accounts.json");
    accounts["data"][0]["attributes"]["name"] = "Main checking".into();
    accounts["data"][0]["attributes"]["notes"] = "salary account".into();
    // A Firefly account in a currency Saldo does not know is skipped, not fatal.
    accounts["data"][4]["attributes"]["currency_code"] = "XYZ".into();
    let mut categories = fixture("categories.json");
    categories["data"][0]["attributes"]["name"] = "Salary".into();
    // Renaming Housing to Food clashes with the existing Food category.
    categories["data"][2]["attributes"]["name"] = "Food".into();
    let mut tags = fixture("tags.json");
    tags["data"][3]["attributes"]["description"] = "not used yet".into();

    let report = import(
        &app,
        ctx,
        &firefly_with(accounts, categories, tags).await,
        false,
    )
    .await;

    assert_eq!(
        counts(&report, "accounts").updated,
        1,
        "{}",
        report.render()
    );
    assert_eq!(
        counts(&report, "accounts").skipped,
        2,
        "import account and unknown currency"
    );
    assert_eq!(
        counts(&report, "categories"),
        Counts {
            created: 0,
            updated: 1,
            unchanged: 1,
            skipped: 1
        }
    );
    assert_eq!(
        counts(&report, "tags"),
        Counts {
            created: 0,
            updated: 1,
            unchanged: 3,
            skipped: 0
        }
    );
    let (name, notes): (String, Option<String>) =
        sqlx::query_as("SELECT name, notes FROM accounts WHERE name = 'Main checking'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(
        (name.as_str(), notes.as_deref()),
        ("Main checking", Some("salary account"))
    );
    assert!(
        report
            .messages
            .iter()
            .any(|m| m.contains("unknown currency XYZ")),
        "{:?}",
        report.messages
    );
    assert!(
        report
            .messages
            .iter()
            .any(|m| m.starts_with("categories:") && m.contains("exists")),
        "{:?}",
        report.messages
    );
}

#[tokio::test]
async fn budgets_limits_and_budget_links_import_once() {
    let (app, ctx) = household().await;
    // Firefly links: groceries (a withdrawal) to budget 1, the loan payment
    // (a transfer in Saldo) to budget 2, coffee to a budget it did not list.
    let mut page_one = fixture("transactions-1.json");
    let mut page_two = fixture("transactions-2.json");
    page_one["data"][3]["attributes"]["transactions"][0]["budget_id"] = "1".into();
    let groups = page_two["data"].as_array_mut().unwrap();
    let split = |groups: &mut Vec<Value>, id: &str| {
        let group = groups.iter_mut().find(|g| g["id"] == id).unwrap();
        group["attributes"]["transactions"][0].clone()
    };
    assert_eq!(split(groups, "8")["description"], "Loan payment");
    for (id, budget) in [("8", "2"), ("6", "99")] {
        let group = groups.iter_mut().find(|g| g["id"] == id).unwrap();
        group["attributes"]["transactions"][0]["budget_id"] = budget.into();
    }
    let server = firefly(Some(page_one), Some(page_two)).await;

    let first = import(&app, ctx, &server, false).await;

    assert_eq!(counts(&first, "budgets").created, 2);
    assert_eq!(counts(&first, "budget_limits").created, 2);
    assert_eq!(
        first.notes["budget on a split that is not a withdrawal in Saldo dropped"],
        1
    );
    assert_eq!(first.notes["budget not imported, split kept without it"], 1);
    let groceries = scalar(&app, "SELECT count(*) FROM splits s JOIN budgets b ON b.id = s.budget_id WHERE b.name = 'Groceries' AND s.description = 'Groceries'").await;
    assert_eq!(groceries, 1);
    assert_eq!(
        scalar(
            &app,
            "SELECT count(*) FROM splits WHERE budget_id IS NOT NULL"
        )
        .await,
        1
    );
    let auto: (Option<String>, Option<String>, Option<i64>) = sqlx::query_as(
        "SELECT auto_type, auto_period, auto_amount_minor FROM budgets WHERE name = 'Groceries'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(
        auto,
        (Some("reset".into()), Some("monthly".into()), Some(30_000))
    );
    let october: (String, String, i64, Option<String>) = sqlx::query_as(
        "SELECT start_date, end_date, amount_minor, notes FROM budget_limits ORDER BY start_date LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(
        october,
        (
            "2026-10-01".into(),
            "2026-10-31".into(),
            25_000,
            Some("October".into())
        )
    );
    // The import itself never creates automatic limits.
    assert_eq!(scalar(&app, "SELECT count(*) FROM budget_limits").await, 2);

    let before = snapshot(&app).await;
    let second = import(&app, ctx, &server, false).await;
    assert!(!second.has_changes(), "{}", second.render());
    assert_eq!(counts(&second, "budgets").unchanged, 2);
    assert_eq!(counts(&second, "budget_limits").unchanged, 2);
    assert_eq!(
        snapshot(&app).await,
        before,
        "the database must be identical"
    );
}

#[tokio::test]
async fn changed_budgets_are_updated() {
    let (app, ctx) = household().await;
    import(&app, ctx, &firefly(None, None).await, false).await;

    let mut budgets = fixture("budgets.json");
    budgets["data"][0]["attributes"]["auto_budget_type"] = "adjusted".into();
    budgets["data"][1]["attributes"]["name"] = "Leisure".into();
    let server = firefly(None, None).await;
    // Priority 1 wins over the fixture mounted by `firefly()` (default 5).
    Mock::given(method("GET"))
        .and(path("/api/v1/budgets"))
        .respond_with(ResponseTemplate::new(200).set_body_json(budgets))
        .with_priority(1)
        .mount(&server)
        .await;
    let options = Options {
        only: Some(vec!["budgets".to_string()]),
        ..Options::default()
    };
    let client = FireflyClient::new(&server.uri(), "secret-token".into()).unwrap();
    let report = run(app.clone(), client, ctx, options).await.unwrap();

    assert_eq!(counts(&report, "budgets").updated, 2);
    assert_eq!(
        report.notes["auto-budget type adjusted not supported, imported without it"],
        1
    );
    assert_eq!(
        scalar(&app, "SELECT count(*) FROM budgets WHERE name = 'Leisure'").await,
        1
    );
    assert_eq!(
        scalar(
            &app,
            "SELECT count(*) FROM budgets WHERE auto_type IS NOT NULL"
        )
        .await,
        0
    );
}

/// Firefly stores a rollover budget's limits with the leftover added; Saldo
/// stores the base and computes the carry (budget carry `leftover`).
#[tokio::test]
async fn rollover_budgets_import_with_a_carry_and_base_limits() {
    let (app, ctx) = household().await;
    let mut budgets = fixture("budgets.json");
    budgets["data"][0]["attributes"]["auto_budget_type"] = "rollover".into();
    budgets["data"][0]["attributes"]["auto_budget_amount"] = "200.000000000000".into();
    let server = firefly(None, None).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/budgets"))
        .respond_with(ResponseTemplate::new(200).set_body_json(budgets))
        .with_priority(1)
        .mount(&server)
        .await;
    let options = || Options {
        only: Some(vec!["budgets".to_string()]),
        ..Options::default()
    };
    let client = || FireflyClient::new(&server.uri(), "secret-token".into()).unwrap();
    run(app.clone(), client(), ctx, options()).await.unwrap();

    assert_eq!(
        scalar(
            &app,
            "SELECT count(*) FROM budgets WHERE name = 'Groceries' AND auto_type = 'reset' AND carry = 'leftover'"
        )
        .await,
        1
    );
    // Both fixture limits (300 and 250) were above the 200 base.
    assert_eq!(
        scalar(
            &app,
            "SELECT count(*) FROM budget_limits WHERE amount_minor = 20000"
        )
        .await,
        2
    );

    let second = run(app.clone(), client(), ctx, options()).await.unwrap();
    assert!(!second.has_changes(), "{}", second.render());
}

/// The fixture's tags plus Firefly tag 5, "FOOD", a case duplicate of "food".
fn tags_with_case_duplicate() -> Value {
    let mut tags = fixture("tags.json");
    let mut duplicate = tags["data"][2].clone();
    duplicate["id"] = "5".into();
    duplicate["attributes"]["tag"] = "FOOD".into();
    tags["data"].as_array_mut().unwrap().push(duplicate);
    tags
}

async fn tag_names(app: &App) -> Vec<String> {
    sqlx::query_scalar("SELECT name FROM tags ORDER BY name")
        .fetch_all(&app.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn firefly_tags_differing_in_case_import_as_one() {
    let (app, ctx) = household().await;
    let server = firefly_with(
        fixture("accounts.json"),
        fixture("categories.json"),
        tags_with_case_duplicate(),
    )
    .await;

    let report = import(&app, ctx, &server, false).await;

    let tags = counts(&report, "tags");
    assert_eq!(
        (tags.created, tags.unchanged),
        (4, 1),
        "{}",
        report.render()
    );
    assert_eq!(
        tag_names(&app).await,
        ["food", "salary", "unused", "weekly"]
    );
    assert_eq!(
        scalar(
            &app,
            "SELECT count(DISTINCT local_id) FROM external_refs WHERE entity_type = 'tag'"
        )
        .await,
        4
    );
    assert_eq!(scalar(&app, "SELECT count(*) FROM split_tags").await, 4);
}

#[tokio::test]
async fn a_tag_recased_in_saldo_keeps_its_name_on_reimport() {
    let (app, ctx) = household().await;
    import(&app, ctx, &firefly(None, None).await, false).await;
    let food: String = sqlx::query_scalar("SELECT id FROM tags WHERE name = 'food'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let rename = saldo_app::tags::TagUpdate {
        name: Some("Food".into()),
        description: None,
    };
    app.tags
        .update(&ctx, Origin::Api, food.parse().unwrap(), rename)
        .await
        .unwrap();
    let server = firefly_with(
        fixture("accounts.json"),
        fixture("categories.json"),
        tags_with_case_duplicate(),
    )
    .await;

    let report = import(&app, ctx, &server, false).await;

    let tags = counts(&report, "tags");
    assert_eq!(
        (tags.updated, tags.unchanged),
        (0, 5),
        "{}",
        report.render()
    );
    assert_eq!(
        tag_names(&app).await,
        ["Food", "salary", "unused", "weekly"]
    );
    assert_eq!(
        counts(&report, "transactions").updated,
        0,
        "{}",
        report.render()
    );
    assert_eq!(scalar(&app, "SELECT count(*) FROM split_tags").await, 4);
}
