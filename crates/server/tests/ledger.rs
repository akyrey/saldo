#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use saldo_app::ctx::{HouseholdCtx, Role};
use saldo_core::{HouseholdId, UserId};
use serde_json::{Value, json};

async fn create(server: &TestServer, auth: &Auth, path: &str, body: Value) -> Value {
    let response = server.post(path, body, auth).await;
    assert_eq!(
        response.status,
        StatusCode::CREATED,
        "{path}: {}",
        response.body
    );
    response.body["data"].clone()
}

async fn account(server: &TestServer, auth: &Auth, name: &str, account_type: &str) -> String {
    let body = create(
        server,
        auth,
        "/api/v1/accounts",
        json!({ "name": name, "type": account_type }),
    )
    .await;
    body["id"].as_str().unwrap().to_string()
}

fn split(date: &str, source: &str, destination: &str, amount: &str) -> Value {
    json!({ "date": date, "description": "test", "source_id": source, "destination_id": destination, "amount": amount })
}

/// The flow from the Phase 2 goal, end to end over HTTP.
#[tokio::test]
async fn setup_login_member_accounts_transaction_balances_events() {
    let server = server().await;
    let owner = Auth::Cookie(server.setup_and_login().await);
    let member =
        json!({ "email": "partner@example.com", "display_name": "Partner", "password": PASSWORD });
    create(&server, &owner, "/api/v1/household/members", member).await;

    let checking = account(&server, &owner, "Checking", "asset").await;
    let card = account(&server, &owner, "Credit card", "liability").await;
    let groceries = account(&server, &owner, "Groceries", "expense").await;
    let group = create(&server, &owner, "/api/v1/transactions", json!({
        "title": "Saturday shop",
        "splits": [split("2026-11-07", &checking, &groceries, "42.50"), split("2026-11-07", &card, &groceries, "17.25")]
    }))
    .await;
    let kinds: Vec<_> = group["splits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["kind"].clone())
        .collect();
    assert_eq!(kinds, [json!("withdrawal"), json!("withdrawal")]);

    let balance = |id: String| {
        let server = &server;
        let owner = owner.clone();
        async move {
            server
                .get(&format!("/api/v1/accounts/{id}/balance"), &owner)
                .await
                .body["data"]["balance"]
                .clone()
        }
    };
    assert_eq!(balance(checking.clone()).await, "-42.50");
    assert_eq!(balance(card.clone()).await, "-17.25");
    assert_eq!(balance(groceries.clone()).await, "59.75");

    let events = server
        .get("/api/v1/events?type=transaction.created", &owner)
        .await;
    assert_eq!(events.body["meta"]["total"], 1);
    let event = &events.body["data"][0];
    assert_eq!(event["entity_id"], group["id"]);
    let fetched = server
        .get(
            &format!("/api/v1/transactions/{}", group["id"].as_str().unwrap()),
            &owner,
        )
        .await;
    assert_eq!(
        event["payload"]["data"], fetched.body["data"],
        "event data is the entity as the API returns it"
    );

    let all = server.get("/api/v1/events?per_page=200", &owner).await;
    let types: Vec<_> = all.body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["type"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(types.iter().filter(|t| *t == "account.created").count(), 5);
    assert!(types.contains(&"household_member.created".to_string()));
    let after = all.body["data"][3]["seq"].as_i64().unwrap();
    let later = server
        .get(&format!("/api/v1/events?after_seq={after}"), &owner)
        .await;
    assert!(
        later.body["data"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["seq"].as_i64().unwrap() > after)
    );
}

#[tokio::test]
async fn failed_validation_is_422_with_core_codes_and_writes_nothing() {
    let server = server().await;
    let owner = Auth::Cookie(server.setup_and_login().await);
    let checking = account(&server, &owner, "Checking", "asset").await;
    let groceries = account(&server, &owner, "Groceries", "expense").await;
    let events_before = server.get("/api/v1/events", &owner).await.body["meta"]["total"].clone();

    let response = server.post("/api/v1/transactions", json!({
        "splits": [split("2026-11-01", &checking, &groceries, "5"), split("2026-11-01", &groceries, &checking, "0")]
    }), &owner).await;

    assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response.body["error"]["code"], "validation_failed");
    let details = response.body["error"]["details"].as_array().unwrap();
    assert_eq!(details[0]["code"], "amount_not_positive");
    assert_eq!(details[0]["field"], "splits[1].amount");
    assert_eq!(details[1]["code"], "invalid_account_pair");
    assert_eq!(
        server.get("/api/v1/transactions", &owner).await.body["meta"]["total"],
        0
    );
    assert_eq!(
        server.get("/api/v1/events", &owner).await.body["meta"]["total"],
        events_before
    );

    let bad_amount = server
        .post(
            "/api/v1/transactions",
            json!({ "splits": [split("2026-11-01", &checking, &groceries, "1,5")] }),
            &owner,
        )
        .await;
    assert_eq!(bad_amount.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        bad_amount.body["error"]["details"][0]["field"],
        "splits[0].amount"
    );
}

#[tokio::test]
async fn accounts_crud_over_http() {
    let server = server().await;
    let owner = Auth::Cookie(server.setup_and_login().await);
    let created = create(&server, &owner, "/api/v1/accounts", json!({ "name": "Wallet", "type": "asset", "currency": "USD", "notes": "cash", "virtual_balance": "10" })).await;
    assert_eq!(
        (
            created["currency"].as_str(),
            created["virtual_balance"].as_str()
        ),
        (Some("USD"), Some("10.00"))
    );
    let path = format!("/api/v1/accounts/{}", created["id"].as_str().unwrap());

    let patched = server
        .send(
            Method::PATCH,
            &path,
            Some(json!({ "name": "Pocket", "notes": null })),
            &owner,
        )
        .await;
    assert_eq!(patched.status, StatusCode::OK);
    assert_eq!(
        (
            patched.body["data"]["name"].as_str(),
            &patched.body["data"]["notes"]
        ),
        (Some("Pocket"), &Value::Null)
    );
    let retype = server
        .send(
            Method::PATCH,
            &path,
            Some(json!({ "type": "expense" })),
            &owner,
        )
        .await;
    assert_eq!(retype.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(retype.body["error"]["details"][0]["field"], "type");

    let duplicate = server
        .post(
            "/api/v1/accounts",
            json!({ "name": "Pocket", "type": "asset" }),
            &owner,
        )
        .await;
    assert_eq!(duplicate.status, StatusCode::CONFLICT);
    let list = server
        .get(
            "/api/v1/accounts?type=asset&scope=household&per_page=10",
            &owner,
        )
        .await;
    assert_eq!(
        (
            list.body["meta"]["total"].as_i64(),
            list.body["meta"]["per_page"].as_i64()
        ),
        (Some(1), Some(10))
    );
    assert_eq!(
        server.get("/api/v1/accounts?scope=mine", &owner).await.body["meta"]["total"],
        0
    );

    assert_eq!(
        server
            .send(Method::DELETE, &path, None, &owner)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        server.get(&path, &owner).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn bad_query_strings_are_rejected() {
    let server = server().await;
    let owner = Auth::Cookie(server.setup_and_login().await);
    assert_eq!(
        server
            .get("/api/v1/accounts?per_page=500", &owner)
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        server
            .get("/api/v1/accounts?scope=everyone", &owner)
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        server
            .get("/api/v1/transactions?from=yesterday", &owner)
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        server
            .get("/api/v1/transactions?kind=refund", &owner)
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        server
            .get("/api/v1/accounts/not-a-uuid", &owner)
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn transactions_filter_replace_and_delete() {
    let server = server().await;
    let owner = Auth::Cookie(server.setup_and_login().await);
    let checking = account(&server, &owner, "Checking", "asset").await;
    let salary = account(&server, &owner, "Salary", "revenue").await;
    let food = account(&server, &owner, "Food", "expense").await;
    let category = create(
        &server,
        &owner,
        "/api/v1/categories",
        json!({ "name": "Groceries" }),
    )
    .await;
    let tag = create(&server, &owner, "/api/v1/tags", json!({ "name": "weekly" })).await;
    create(
        &server,
        &owner,
        "/api/v1/transactions",
        json!({ "splits": [split("2026-11-01", &salary, &checking, "2000")] }),
    )
    .await;
    let mut shop = split("2026-11-05", &checking, &food, "30");
    shop["category_id"] = category["id"].clone();
    shop["tag_ids"] = json!([tag["id"]]);
    shop["description"] = json!("Bakery");
    let group = create(
        &server,
        &owner,
        "/api/v1/transactions",
        json!({ "splits": [shop] }),
    )
    .await;

    let total = |query: String| {
        let server = &server;
        let owner = owner.clone();
        async move {
            server
                .get(&format!("/api/v1/transactions?{query}"), &owner)
                .await
                .body["meta"]["total"]
                .as_i64()
                .unwrap()
        }
    };
    assert_eq!(total("from=2026-11-02".into()).await, 1);
    assert_eq!(total("kind=deposit".into()).await, 1);
    assert_eq!(total("q=bakery".into()).await, 1);
    assert_eq!(
        total(format!("category_id={}", category["id"].as_str().unwrap())).await,
        1
    );
    assert_eq!(
        total(format!("tag_id={}", tag["id"].as_str().unwrap())).await,
        1
    );
    assert_eq!(total(format!("account_id={checking}")).await, 2);

    let path = format!("/api/v1/transactions/{}", group["id"].as_str().unwrap());
    let replaced = server.send(Method::PUT, &path, Some(json!({ "title": "Fixed", "splits": [split("2026-11-06", &checking, &food, "31")] })), &owner).await;
    assert_eq!(replaced.status, StatusCode::OK);
    assert_eq!(replaced.body["data"]["splits"][0]["amount"], "31.00");
    assert_eq!(
        server
            .send(Method::DELETE, &path, None, &owner)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        server.get(&path, &owner).await.status,
        StatusCode::NOT_FOUND
    );
    let last = server
        .get("/api/v1/events?type=transaction.deleted", &owner)
        .await;
    assert_eq!(last.body["data"][0]["payload"]["data"]["title"], "Fixed");
}

#[tokio::test]
async fn categories_tags_and_currencies_over_http() {
    let server = server().await;
    let owner = Auth::Cookie(server.setup_and_login().await);
    for (path, field) in [
        ("/api/v1/categories", "notes"),
        ("/api/v1/tags", "description"),
    ] {
        let created = create(&server, &owner, path, json!({ "name": "One", field: "x" })).await;
        let item = format!("{path}/{}", created["id"].as_str().unwrap());
        let patched = server
            .send(
                Method::PATCH,
                &item,
                Some(json!({ "name": "Two", field: null })),
                &owner,
            )
            .await;
        assert_eq!(
            (
                patched.body["data"]["name"].as_str(),
                &patched.body["data"][field]
            ),
            (Some("Two"), &Value::Null),
            "{path}"
        );
        assert_eq!(server.get(&item, &owner).await.body["data"]["name"], "Two");
        assert_eq!(
            server
                .post(path, json!({ "name": "Two" }), &owner)
                .await
                .status,
            StatusCode::CONFLICT
        );
        assert_eq!(server.get(path, &owner).await.body["meta"]["total"], 1);
        assert_eq!(
            server
                .send(Method::DELETE, &item, None, &owner)
                .await
                .status,
            StatusCode::NO_CONTENT
        );
    }
    let currencies = server.get("/api/v1/currencies", &owner).await;
    assert!(
        currencies.body["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["code"] == "EUR")
    );
}

#[tokio::test]
async fn another_household_gets_404s_over_http() {
    let server = server().await;
    let owner = Auth::Cookie(server.setup_and_login().await);
    let checking = account(&server, &owner, "Checking", "asset").await;
    let food = account(&server, &owner, "Food", "expense").await;
    let group = create(
        &server,
        &owner,
        "/api/v1/transactions",
        json!({ "splits": [split("2026-11-01", &checking, &food, "5")] }),
    )
    .await;

    // A second household, created directly, with its own API token.
    let ctx = HouseholdCtx {
        household_id: HouseholdId(uuid::Uuid::from_u128(0xB0B)),
        user_id: UserId(uuid::Uuid::from_u128(0xB0B1)),
        role: Role::Owner,
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
            .execute(&server.app.pool)
            .await
            .unwrap();
    }
    let other = Auth::Bearer(
        server
            .app
            .identity
            .create_token(&ctx, "b")
            .await
            .unwrap()
            .token,
    );

    for path in [
        format!("/api/v1/accounts/{checking}"),
        format!("/api/v1/accounts/{checking}/balance"),
        format!("/api/v1/transactions/{}", group["id"].as_str().unwrap()),
    ] {
        assert_eq!(
            server.get(&path, &other).await.status,
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    let delete = server
        .send(
            Method::DELETE,
            &format!("/api/v1/accounts/{food}"),
            None,
            &other,
        )
        .await;
    assert_eq!(delete.status, StatusCode::NOT_FOUND);
    assert_eq!(
        server.get("/api/v1/transactions", &other).await.body["meta"]["total"],
        0
    );
    assert_eq!(
        server.get("/api/v1/events", &other).await.body["meta"]["total"],
        0
    );
    let steal = server
        .post(
            "/api/v1/transactions",
            json!({ "splits": [split("2026-11-01", &checking, &food, "5")] }),
            &other,
        )
        .await;
    assert_eq!(steal.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(steal.body["error"]["details"][0]["code"], "unknown_account");
}
