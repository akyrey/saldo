#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use saldo_server::config::Config;
use saldo_server::state::AppState;
use saldo_server::workers::recurrences::run_once;
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
    let body = json!({ "name": name, "type": account_type });
    create(server, auth, "/api/v1/accounts", body).await["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn fund(server: &TestServer, auth: &Auth, account: &str, amount: &str) {
    let salary = account_named(server, auth, "Salary", "revenue").await;
    let split = json!({ "date": "2026-11-01", "description": "pay", "source_id": salary,
        "destination_id": account, "amount": amount });
    create(
        server,
        auth,
        "/api/v1/transactions",
        json!({ "splits": [split] }),
    )
    .await;
}

async fn account_named(server: &TestServer, auth: &Auth, name: &str, kind: &str) -> String {
    let list = server.get("/api/v1/accounts?per_page=200", auth).await;
    let found = list.body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == name)
        .map(|a| a["id"].as_str().unwrap().to_string());
    match found {
        Some(id) => id,
        None => account(server, auth, name, kind).await,
    }
}

#[tokio::test]
async fn piggy_bank_crud_and_money_over_http() {
    let server = server().await;
    let auth = Auth::Cookie(server.setup_and_login().await);
    let savings = account(&server, &auth, "Savings", "asset").await;
    fund(&server, &auth, &savings, "100.00").await;

    let piggy = create(
        &server,
        &auth,
        "/api/v1/piggy-banks",
        json!({
            "name": "Bike", "target_amount": "80.00", "account_ids": [savings]
        }),
    )
    .await;
    let id = piggy["id"].as_str().unwrap();
    assert_eq!(piggy["saved_amount"], "0.00");

    let event = create(
        &server,
        &auth,
        &format!("/api/v1/piggy-banks/{id}/events"),
        json!({ "amount": "80.00", "date": "2026-11-13" }),
    )
    .await;
    assert_eq!(event["amount"], "80.00");
    let too_much = server
        .post(
            &format!("/api/v1/piggy-banks/{id}/events"),
            json!({ "amount": "0.01", "date": "2026-11-13" }),
            &auth,
        )
        .await;
    assert_eq!(too_much.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(too_much.body["error"]["details"][0]["field"], "amount");

    let events = server
        .get(&format!("/api/v1/piggy-banks/{id}/events"), &auth)
        .await;
    assert_eq!(events.body["meta"]["total"], 1);
    let patched = server
        .send(
            Method::PATCH,
            &format!("/api/v1/piggy-banks/{id}"),
            Some(json!({ "name": "E-bike", "target_amount": null })),
            &auth,
        )
        .await;
    assert_eq!(patched.status, StatusCode::OK, "{}", patched.body);
    assert_eq!(patched.body["data"]["target_amount"], Value::Null);
    let list = server.get("/api/v1/piggy-banks", &auth).await;
    assert_eq!(list.body["data"][0]["saved_amount"], "80.00");

    let types = server
        .get("/api/v1/events?type=piggy_bank.target_reached", &auth)
        .await;
    assert_eq!(types.body["meta"]["total"], 1);
    let deleted = server
        .send(
            Method::DELETE,
            &format!("/api/v1/piggy-banks/{id}"),
            None,
            &auth,
        )
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let gone = server
        .get(&format!("/api/v1/piggy-banks/{id}"), &auth)
        .await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
}

fn recurrence(checking: &str, landlord: &str) -> Value {
    json!({
        "title": "Rent",
        "first_date": "2026-11-01",
        "repetitions": [{ "kind": "monthly", "moment": "1" }],
        "splits": [{ "description": "Rent", "source_id": checking, "destination_id": landlord, "amount": "800.00" }]
    })
}

#[tokio::test]
async fn recurrence_crud_fire_and_preview_over_http() {
    let server = server().await;
    let auth = Auth::Cookie(server.setup_and_login().await);
    let checking = account(&server, &auth, "Checking", "asset").await;
    let landlord = account(&server, &auth, "Landlord", "expense").await;

    let created = create(
        &server,
        &auth,
        "/api/v1/recurrences",
        recurrence(&checking, &landlord),
    )
    .await;
    let id = created["id"].as_str().unwrap();
    assert_eq!(created["repetitions"][0]["weekend_policy"], "keep");

    let fire = |date: &'static str| {
        let path = format!("/api/v1/recurrences/{id}/fire");
        let server = &server;
        let auth = &auth;
        async move { server.post(&path, json!({ "date": date }), auth).await }
    };
    let first = fire("2026-11-01").await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.body);
    assert_eq!(first.body["data"]["created"], true);
    let again = fire("2026-11-01").await;
    assert_eq!(again.status, StatusCode::OK);
    assert_eq!(
        again.body["data"]["group"]["id"],
        first.body["data"]["group"]["id"]
    );
    let transactions = server.get("/api/v1/transactions", &auth).await;
    assert_eq!(transactions.body["meta"]["total"], 1);

    // Without a date: the household's today (the test clock says 2026-11-13).
    let today = server
        .post(&format!("/api/v1/recurrences/{id}/fire"), json!({}), &auth)
        .await;
    assert_eq!(today.body["data"]["date"], "2026-11-13");

    let preview = server
        .post(
            "/api/v1/recurrences/preview",
            json!({
                "first_date": "2026-11-01", "count": 2,
                "repetitions": [{ "kind": "nth_weekday", "moment": "2,3" }]
            }),
            &auth,
        )
        .await;
    assert_eq!(preview.status, StatusCode::OK, "{}", preview.body);
    assert_eq!(preview.body["data"], json!(["2026-12-09", "2027-01-13"]));
    let bad = server
        .post(
            "/api/v1/recurrences/preview",
            json!({
                "first_date": "2026-11-01", "repetitions": [{ "kind": "monthly", "moment": "40" }]
            }),
            &auth,
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        bad.body["error"]["details"][0]["field"],
        "repetitions[0].moment"
    );

    let mut body = recurrence(&checking, &landlord);
    body["title"] = json!("Flat");
    let replaced = server
        .send(
            Method::PUT,
            &format!("/api/v1/recurrences/{id}"),
            Some(body),
            &auth,
        )
        .await;
    assert_eq!(replaced.status, StatusCode::OK, "{}", replaced.body);
    assert_eq!(replaced.body["data"]["title"], "Flat");
    let list = server.get("/api/v1/recurrences", &auth).await;
    assert_eq!(list.body["meta"]["total"], 1);
    let deleted = server
        .send(
            Method::DELETE,
            &format!("/api/v1/recurrences/{id}"),
            None,
            &auth,
        )
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn the_runner_fires_due_recurrences_and_wakes_listeners() {
    let server = server().await;
    let auth = Auth::Cookie(server.setup_and_login().await);
    let checking = account(&server, &auth, "Checking", "asset").await;
    let landlord = account(&server, &auth, "Landlord", "expense").await;
    let mut body = recurrence(&checking, &landlord);
    body["first_date"] = json!("2026-09-01");
    create(&server, &auth, "/api/v1/recurrences", body).await;

    let config = Config::from_lookup(|_| None).unwrap();
    let state = AppState::new(server.app.clone(), config);
    let mut woken = state.events.subscribe();
    assert_eq!(
        run_once(&state).await.unwrap(),
        3,
        "September, October, November"
    );
    assert!(woken.has_changed().unwrap());
    woken.mark_unchanged();
    assert_eq!(run_once(&state).await.unwrap(), 0);
    assert!(!woken.has_changed().unwrap());
    let transactions = server.get("/api/v1/transactions", &auth).await;
    assert_eq!(transactions.body["meta"]["total"], 3);
}

#[tokio::test]
async fn new_routes_require_authentication() {
    let server = server().await;
    for path in ["/api/v1/piggy-banks", "/api/v1/recurrences"] {
        assert_eq!(
            server.get(path, &Auth::None).await.status,
            StatusCode::UNAUTHORIZED
        );
    }
}
