#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use axum::http::{Method, StatusCode};
use common::*;
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
    let view = create(server, auth, "/api/v1/accounts", body).await;
    view["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn budgets_limits_and_budgeted_withdrawals_over_http() {
    let server = server().await;
    let auth = Auth::Cookie(server.setup_and_login().await);

    let budget = create(
        &server,
        &auth,
        "/api/v1/budgets",
        json!({
            "name": "Groceries",
            "auto_budget": { "type": "reset", "period": "monthly", "amount": "300" }
        }),
    )
    .await;
    assert_eq!(budget["auto_budget"]["amount"], "300.00");
    assert_eq!(budget["auto_budget"]["currency"], "EUR");
    let id = budget["id"].as_str().unwrap();

    let dup = server
        .post("/api/v1/budgets", json!({ "name": "Groceries" }), &auth)
        .await;
    assert_eq!(dup.status, StatusCode::CONFLICT);
    let bad = server
        .post("/api/v1/budgets", json!({ "name": "X", "auto_budget": { "type": "x", "period": "monthly", "amount": "1" } }), &auth)
        .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(bad.body["error"]["details"][0]["field"], "auto_budget.type");

    let checking = account(&server, &auth, "Checking", "asset").await;
    let shop = account(&server, &auth, "Shop", "expense").await;
    let salary = account(&server, &auth, "Employer", "revenue").await;
    create(
        &server,
        &auth,
        "/api/v1/transactions",
        json!({ "splits": [{
        "date": "2026-11-05", "description": "food", "source_id": checking,
        "destination_id": shop, "amount": "120.00", "budget_id": id
    }] }),
    )
    .await;
    let deposit = server
        .post(
            "/api/v1/transactions",
            json!({ "splits": [{
            "date": "2026-11-05", "description": "pay", "source_id": salary,
            "destination_id": checking, "amount": "1.00", "budget_id": id
        }] }),
            &auth,
        )
        .await;
    assert_eq!(deposit.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        deposit.body["error"]["details"][0]["field"],
        "splits[0].budget_id"
    );

    let limits = server
        .get(
            &format!("/api/v1/budgets/{id}/limits?from=2026-11-01&to=2026-11-30"),
            &auth,
        )
        .await;
    assert_eq!(limits.status, StatusCode::OK);
    assert_eq!(limits.body["meta"]["total"], 1);
    let limit = &limits.body["data"][0];
    assert_eq!(
        (limit["spent"].as_str(), limit["available"].as_str()),
        (Some("120.00"), Some("180.00"))
    );
    let limit_id = limit["id"].as_str().unwrap();

    let patched = server
        .send(
            Method::PATCH,
            &format!("/api/v1/budget-limits/{limit_id}"),
            Some(json!({ "amount": "100", "notes": "tight" })),
            &auth,
        )
        .await;
    assert_eq!(patched.status, StatusCode::OK, "{}", patched.body);
    assert_eq!(patched.body["data"]["available"], "-20.00");

    let december = create(
        &server,
        &auth,
        &format!("/api/v1/budgets/{id}/limits"),
        json!({
            "start_date": "2026-12-01", "end_date": "2026-12-31", "amount": "250.50"
        }),
    )
    .await;
    assert_eq!(december["spent"], "0.00");
    let overlap = server
        .post(
            &format!("/api/v1/budgets/{id}/limits"),
            json!({ "start_date": "2026-12-15", "end_date": "2026-12-16", "amount": "1" }),
            &auth,
        )
        .await;
    assert_eq!(overlap.status, StatusCode::CONFLICT);

    let deleted = server
        .send(
            Method::DELETE,
            &format!("/api/v1/budget-limits/{}", december["id"].as_str().unwrap()),
            None,
            &auth,
        )
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);

    let renamed = server
        .send(
            Method::PATCH,
            &format!("/api/v1/budgets/{id}"),
            Some(json!({ "name": "Food", "auto_budget": null })),
            &auth,
        )
        .await;
    assert_eq!(renamed.status, StatusCode::OK);
    assert_eq!(renamed.body["data"]["auto_budget"], Value::Null);
    let list = server.get("/api/v1/budgets?active=true", &auth).await;
    assert_eq!(list.body["data"][0]["name"], "Food");

    let gone = server
        .send(
            Method::DELETE,
            &format!("/api/v1/budgets/{id}"),
            None,
            &auth,
        )
        .await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT);
    let missing = server.get(&format!("/api/v1/budgets/{id}"), &auth).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    let filtered = server
        .get(&format!("/api/v1/transactions?budget_id={id}"), &auth)
        .await;
    assert_eq!(filtered.body["meta"]["total"], 0);
}
