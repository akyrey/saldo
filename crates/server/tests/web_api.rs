//! Endpoints added in Phase 4 for the web app: the setup status and the
//! balances of many accounts in one request.

#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::{Value, json};

async fn account(server: &TestServer, auth: &Auth, name: &str, account_type: &str) -> String {
    let response = server
        .post(
            "/api/v1/accounts",
            json!({ "name": name, "type": account_type }),
            auth,
        )
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    response.body["data"]["id"].as_str().unwrap().to_string()
}

async fn transfer(
    server: &TestServer,
    auth: &Auth,
    date: &str,
    from: &str,
    to: &str,
    amount: &str,
) {
    let body = json!({ "splits": [
        { "date": date, "description": "t", "source_id": from, "destination_id": to, "amount": amount }
    ]});
    let response = server.post("/api/v1/transactions", body, auth).await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
}

fn balance_of<'a>(body: &'a Value, id: &str) -> &'a Value {
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["account_id"] == id)
        .map(|b| &b["balance"])
        .unwrap_or(&Value::Null)
}

#[tokio::test]
async fn setup_status_is_public_and_flips_after_setup() {
    let server = server().await;
    let before = server.get("/api/v1/setup", &Auth::None).await;
    assert_eq!(before.status, StatusCode::OK);
    assert_eq!(before.body, json!({ "data": { "needed": true } }));

    server.setup_and_login().await;
    let after = server.get("/api/v1/setup", &Auth::None).await;
    assert_eq!(after.body, json!({ "data": { "needed": false } }));
}

#[tokio::test]
async fn balances_follow_the_account_list_filters() {
    let server = server().await;
    let owner = Auth::Cookie(server.setup_and_login().await);
    let checking = account(&server, &owner, "Checking", "asset").await;
    let savings = account(&server, &owner, "Savings", "asset").await;
    let food = account(&server, &owner, "Food", "expense").await;
    transfer(&server, &owner, "2026-11-01", &checking, &food, "10.00").await;
    transfer(&server, &owner, "2026-11-05", &checking, &savings, "2.50").await;

    let all = server
        .get("/api/v1/accounts/balances?per_page=200", &owner)
        .await;
    assert_eq!(all.status, StatusCode::OK, "{}", all.body);
    assert_eq!(balance_of(&all.body, &checking), "-12.50");
    assert_eq!(balance_of(&all.body, &savings), "2.50");
    assert_eq!(balance_of(&all.body, &food), "10.00");
    let listed = server.get("/api/v1/accounts?per_page=200", &owner).await;
    assert_eq!(
        all.body["meta"], listed.body["meta"],
        "same rows as /accounts"
    );

    let assets = server
        .get(
            "/api/v1/accounts/balances?type=asset&date=2026-11-02",
            &owner,
        )
        .await;
    assert_eq!(assets.body["meta"]["total"], 2);
    assert_eq!(balance_of(&assets.body, &checking), "-10.00");
    assert_eq!(assets.body["data"][0]["date"], "2026-11-02");
    assert_eq!(balance_of(&assets.body, &food), &Value::Null);
}

#[tokio::test]
async fn balances_need_auth_and_valid_filters() {
    let server = server().await;
    let owner = Auth::Cookie(server.setup_and_login().await);
    let anonymous = server.get("/api/v1/accounts/balances", &Auth::None).await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    let bad = server
        .get("/api/v1/accounts/balances?type=planet", &owner)
        .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
}
