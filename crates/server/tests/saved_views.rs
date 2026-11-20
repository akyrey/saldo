#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use serde_json::json;

#[tokio::test]
async fn saved_view_crud_over_http() {
    let server = server().await;
    let auth = Auth::Cookie(server.setup_and_login().await);

    let created = server
        .post(
            "/api/v1/saved-views",
            json!({ "name": "Inbox", "filters": { "category_id": "none" } }),
            &auth,
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let id = created.body["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(created.body["data"]["page"], "transactions");

    let again = server
        .post(
            "/api/v1/saved-views",
            json!({ "name": "Inbox", "filters": {} }),
            &auth,
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    let nested = server
        .post(
            "/api/v1/saved-views",
            json!({ "name": "Bad", "filters": { "a": [1] } }),
            &auth,
        )
        .await;
    assert_eq!(nested.status, StatusCode::UNPROCESSABLE_ENTITY);

    let path = format!("/api/v1/saved-views/{id}");
    let patched = server
        .send(
            Method::PATCH,
            &path,
            Some(json!({ "name": "Uncategorised" })),
            &auth,
        )
        .await;
    assert_eq!(patched.status, StatusCode::OK);
    assert_eq!(patched.body["data"]["filters"]["category_id"], "none");

    let list = server
        .get("/api/v1/saved-views?page=transactions", &auth)
        .await;
    assert_eq!(list.body["data"][0]["name"], "Uncategorised");
    let wrong_page = server.get("/api/v1/saved-views?page=accounts", &auth).await;
    assert_eq!(wrong_page.status, StatusCode::UNPROCESSABLE_ENTITY);

    let deleted = server.send(Method::DELETE, &path, None, &auth).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let gone = server.send(Method::DELETE, &path, None, &auth).await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
}
