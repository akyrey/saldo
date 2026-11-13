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
    create(server, auth, "/api/v1/accounts", body).await["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn template_crud_over_http() {
    let server = server().await;
    let auth = Auth::Cookie(server.setup_and_login().await);
    let bank = account(&server, &auth, "Bank", "asset").await;

    let template = create(
        &server,
        &auth,
        "/api/v1/transaction-templates",
        json!({ "name": "Coffee", "kind": "withdrawal", "title": "Morning",
                "splits": [{ "source_id": bank, "amount": "2.5" }, {}] }),
    )
    .await;
    let id = template["id"].as_str().unwrap();
    assert_eq!(template["splits"][0]["amount"], "2.50");
    assert_eq!(template["splits"][1]["source_id"], Value::Null);

    let duplicate = server
        .post(
            "/api/v1/transaction-templates",
            json!({ "name": "Coffee", "kind": "deposit" }),
            &auth,
        )
        .await;
    assert_eq!(duplicate.status, StatusCode::CONFLICT);
    let bad = server
        .post(
            "/api/v1/transaction-templates",
            json!({ "name": "Bad", "kind": "withdrawal", "splits": [{ "amount": "x" }] }),
            &auth,
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(bad.body["error"]["details"][0]["field"], "splits[0].amount");

    let path = format!("/api/v1/transaction-templates/{id}");
    let patched = server
        .send(
            Method::PATCH,
            &path,
            Some(json!({ "name": "Tea", "title": null, "sort_order": 4 })),
            &auth,
        )
        .await;
    assert_eq!(patched.status, StatusCode::OK, "{}", patched.body);
    assert_eq!(patched.body["data"]["name"], "Tea");
    assert_eq!(patched.body["data"]["title"], Value::Null);
    assert_eq!(patched.body["data"]["splits"][0]["amount"], "2.50");

    let list = server.get("/api/v1/transaction-templates", &auth).await;
    assert_eq!(list.status, StatusCode::OK);
    assert_eq!(list.body["meta"]["total"], 1);
    assert_eq!(server.get(&path, &auth).await.body["data"]["sort_order"], 4);

    let deleted = server.send(Method::DELETE, &path, None, &auth).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    assert_eq!(server.get(&path, &auth).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_transaction_is_saved_as_a_template_over_http() {
    let server = server().await;
    let auth = Auth::Cookie(server.setup_and_login().await);
    let bank = account(&server, &auth, "Bank", "asset").await;
    let shop = account(&server, &auth, "Shop", "expense").await;
    let split = json!({ "date": "2026-11-02", "description": "Bread", "source_id": bank,
        "destination_id": shop, "amount": "3.20", "reconciled": true });
    let group = create(
        &server,
        &auth,
        "/api/v1/transactions",
        json!({ "splits": [split] }),
    )
    .await;
    let group_id = group["id"].as_str().unwrap();

    let template = create(
        &server,
        &auth,
        &format!("/api/v1/transactions/{group_id}/template"),
        json!({ "name": "Bread" }),
    )
    .await;
    assert_eq!(template["kind"], "withdrawal");
    let saved = &template["splits"][0];
    assert_eq!(saved["description"], "Bread");
    assert_eq!(saved["destination_id"], shop.as_str());
    assert!(saved.get("date").is_none() && saved.get("reconciled").is_none());

    let missing = server
        .post(
            "/api/v1/transactions/0190f5c3-7a2b-7c00-8000-000000000001/template",
            json!({ "name": "X" }),
            &auth,
        )
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}
