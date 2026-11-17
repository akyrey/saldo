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

async fn list(server: &TestServer, auth: &Auth) -> Response {
    server.get("/api/v1/subscription-candidates", auth).await
}

#[tokio::test]
async fn suggestions_can_be_dismissed_restored_and_accepted() {
    let server = server().await;
    let auth = Auth::Bearer(server.setup_and_login().await);
    let account = |name: &str, kind: &str| json!({ "name": name, "type": kind });
    let checking = create(
        &server,
        &auth,
        "/api/v1/accounts",
        account("Checking", "asset"),
    )
    .await;
    let gym = create(
        &server,
        &auth,
        "/api/v1/accounts",
        account("Gym", "expense"),
    )
    .await;
    // Today is 2026-11-13.
    for date in ["2026-08-03", "2026-09-03", "2026-10-03", "2026-11-03"] {
        create(
            &server,
            &auth,
            "/api/v1/transactions",
            json!({ "splits": [{ "date": date, "description": "gym", "source_id": checking["id"],
                                 "destination_id": gym["id"], "amount": "30.00" }] }),
        )
        .await;
    }
    let found = list(&server, &auth).await;
    assert_eq!(found.status, StatusCode::OK, "{}", found.body);
    let candidate = found.body["data"][0].clone();
    assert_eq!(candidate["account_name"], "Gym");
    assert_eq!(candidate["frequency"], "monthly");

    let dismiss = json!({ "account_id": gym["id"], "currency": "EUR" });
    let response = server
        .post("/api/v1/subscription-candidates/dismiss", dismiss, &auth)
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(list(&server, &auth).await.body["data"], json!([]));
    let path = format!(
        "/api/v1/subscription-dismissals/{}/EUR",
        gym["id"].as_str().unwrap()
    );
    let restored = server.send(Method::DELETE, &path, None, &auth).await;
    assert_eq!(restored.status, StatusCode::NO_CONTENT);
    assert_eq!(
        server.send(Method::DELETE, &path, None, &auth).await.status,
        StatusCode::NOT_FOUND
    );

    let accept = json!({
        "account_id": gym["id"], "currency": "EUR", "name": "Gym", "frequency": "monthly",
        "amount_min": "30.00", "amount_max": "30.00", "split_ids": candidate["split_ids"]
    });
    let bill = create(
        &server,
        &auth,
        "/api/v1/subscription-candidates/accept",
        accept,
    )
    .await;
    assert_eq!(bill["first_due_date"], "2026-08-03");
    assert_eq!(list(&server, &auth).await.body["data"], json!([]));

    let bad = json!({
        "account_id": gym["id"], "currency": "EUR", "name": "Again", "frequency": "monthly",
        "amount_min": "30.00", "amount_max": "30.00", "split_ids": candidate["split_ids"]
    });
    let response = server
        .post("/api/v1/subscription-candidates/accept", bad, &auth)
        .await;
    assert_eq!(
        response.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        response.body
    );
}
