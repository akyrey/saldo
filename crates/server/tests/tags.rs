#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::{Value, json};

async fn create_tag(server: &TestServer, auth: &Auth, name: &str) -> String {
    let response = server
        .post("/api/v1/tags", json!({ "name": name }), auth)
        .await;
    assert_eq!(response.status, StatusCode::CREATED, "{}", response.body);
    response.body["data"]["id"].as_str().unwrap().to_string()
}

/// A tag inserted directly: the API refuses names that differ only in case.
async fn legacy_tag(server: &TestServer, name: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO tags (id, household_id, name, created_at, updated_at)
         SELECT ?, id, ?, 't', 't' FROM households",
    )
    .bind(&id)
    .bind(name)
    .execute(&server.app.pool)
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn case_duplicates_are_listed_and_merged_over_http() {
    let server = server().await;
    let auth = Auth::Cookie(server.setup_and_login().await);
    let food = create_tag(&server, &auth, "Food").await;
    create_tag(&server, &auth, "Rent").await;
    let clash = server
        .post("/api/v1/tags", json!({ "name": "FOOD" }), &auth)
        .await;
    assert_eq!(clash.status, StatusCode::CONFLICT);
    let dupe = legacy_tag(&server, "food").await;

    let groups = server.get("/api/v1/tags/duplicates", &auth).await;
    assert_eq!(groups.status, StatusCode::OK, "{}", groups.body);
    let names: Vec<&Value> = groups.body["data"][0]["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| &t["name"])
        .collect();
    assert_eq!(names, [&json!("Food"), &json!("food")]);
    assert_eq!(groups.body["data"][0]["tags"][0]["split_count"], json!(0));

    let merged = server
        .post(
            "/api/v1/tags/merge",
            json!({ "merges": [{ "into": food, "sources": [dupe] }] }),
            &auth,
        )
        .await;
    assert_eq!(merged.status, StatusCode::OK, "{}", merged.body);
    assert_eq!(merged.body["data"][0]["id"], json!(food));
    let gone = server.get(&format!("/api/v1/tags/{dupe}"), &auth).await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
    let none = server.get("/api/v1/tags/duplicates", &auth).await;
    assert_eq!(none.body["data"], json!([]));
}

#[tokio::test]
async fn bad_merges_are_refused() {
    let server = server().await;
    let auth = Auth::Cookie(server.setup_and_login().await);
    let food = create_tag(&server, &auth, "Food").await;
    let unknown = uuid::Uuid::new_v4().to_string();
    for (body, status) in [
        (json!({ "merges": [] }), StatusCode::UNPROCESSABLE_ENTITY),
        (
            json!({ "merges": [{ "into": food, "sources": [food] }] }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({ "merges": [{ "into": food, "sources": [unknown] }] }),
            StatusCode::NOT_FOUND,
        ),
        (
            json!({ "merges": [{ "into": "nope", "sources": [] }] }),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let response = server.post("/api/v1/tags/merge", body.clone(), &auth).await;
        assert_eq!(response.status, status, "{body}: {}", response.body);
    }
    let anonymous = server
        .post("/api/v1/tags/merge", json!({ "merges": [] }), &Auth::None)
        .await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
}
