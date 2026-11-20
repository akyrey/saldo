#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use serde_json::json;

#[tokio::test]
async fn goals_and_safe_to_spend_over_http() {
    let server = server().await;
    let auth = Auth::Cookie(server.setup_and_login().await);

    let created = server
        .post(
            "/api/v1/net-worth-goals",
            json!({ "name": "Emergency fund", "target_amount": "5000", "target_date": "2027-06-30" }),
            &auth,
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let goal = &created.body["data"];
    assert_eq!(goal["start_amount"], "0.00");
    assert_eq!(goal["progress"]["progress_bp"], 0);
    let path = format!("/api/v1/net-worth-goals/{}", goal["id"].as_str().unwrap());

    let patched = server
        .send(
            Method::PATCH,
            &path,
            Some(json!({ "target_date": null })),
            &auth,
        )
        .await;
    assert_eq!(patched.status, StatusCode::OK);
    assert!(patched.body["data"]["target_date"].is_null());
    let bad = server
        .post(
            "/api/v1/net-worth-goals",
            json!({ "name": "Bad", "target_amount": "a lot" }),
            &auth,
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
    let list = server
        .get("/api/v1/net-worth-goals?active=true", &auth)
        .await;
    assert_eq!(list.body["data"].as_array().unwrap().len(), 1);
    let deleted = server.send(Method::DELETE, &path, None, &auth).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);

    let safe = server.get("/api/v1/reports/safe-to-spend", &auth).await;
    assert_eq!(safe.status, StatusCode::OK, "{}", safe.body);
    assert_eq!(safe.body["data"]["safe_to_spend"], "0.00");
    let scoped = server
        .get("/api/v1/reports/safe-to-spend?scope=everyone", &auth)
        .await;
    assert_eq!(scoped.status, StatusCode::UNPROCESSABLE_ENTITY);
}
