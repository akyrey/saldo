#![allow(clippy::unwrap_used)] // helpers outside #[test] functions

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use serde_json::json;

/// Logs the owner in from the desktop app; returns the bearer token.
async fn desktop_login(server: &TestServer) -> String {
    let response = server
        .post(
            "/api/v1/auth/login",
            json!({ "email": "owner@example.com", "password": PASSWORD, "client": "desktop" }),
            &Auth::None,
        )
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    response.body["data"]["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn sessions_are_listed_with_their_client_and_the_current_one_marked() {
    let server = server().await;
    let browser = Auth::Cookie(server.setup_and_login().await);
    let desktop = Auth::Bearer(desktop_login(&server).await);
    server
        .post("/api/v1/auth/tokens", json!({ "name": "script" }), &browser)
        .await;

    let response = server.get("/api/v1/auth/sessions", &browser).await;

    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    let sessions = response.body["data"].as_array().unwrap();
    assert_eq!(sessions.len(), 2, "tokens are not sessions");
    let current: Vec<_> = sessions.iter().filter(|s| s["current"] == true).collect();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0]["client"], "browser");
    let other = sessions.iter().find(|s| s["current"] == false).unwrap();
    assert_eq!(other["client"], "desktop");
    assert!(other["expires_at"].is_string());
    assert!(
        server
            .get("/api/v1/auth/me", &desktop)
            .await
            .status
            .is_success()
    );
}

#[tokio::test]
async fn ending_another_session_logs_that_device_out() {
    let server = server().await;
    let browser = Auth::Cookie(server.setup_and_login().await);
    let desktop = Auth::Bearer(desktop_login(&server).await);
    let sessions = server.get("/api/v1/auth/sessions", &browser).await.body;
    let desktop_id = sessions["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["current"] == false)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let path = format!("/api/v1/auth/sessions/{desktop_id}");
    let ended = server.send(Method::DELETE, &path, None, &browser).await;
    let again = server.send(Method::DELETE, &path, None, &browser).await;

    assert_eq!(ended.status, StatusCode::NO_CONTENT);
    assert_eq!(again.status, StatusCode::NOT_FOUND);
    assert_eq!(
        server.get("/api/v1/auth/me", &desktop).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert!(
        server
            .get("/api/v1/auth/me", &browser)
            .await
            .status
            .is_success()
    );
}

#[tokio::test]
async fn ending_the_current_session_clears_the_cookie() {
    let server = server().await;
    let token = server.setup_and_login().await;
    let browser = Auth::Cookie(token);
    let sessions = server.get("/api/v1/auth/sessions", &browser).await.body;
    let id = sessions["data"][0]["id"].as_str().unwrap().to_string();

    let path = format!("/api/v1/auth/sessions/{id}");
    let response = server.send(Method::DELETE, &path, None, &browser).await;

    assert_eq!(response.status, StatusCode::NO_CONTENT);
    assert_eq!(session_cookie(&response.headers).as_deref(), Some(""));
    assert_eq!(
        server.get("/api/v1/auth/me", &browser).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn revoking_the_other_sessions_keeps_this_one_and_the_tokens() {
    let server = server().await;
    let browser = Auth::Cookie(server.setup_and_login().await);
    let laptop = Auth::Cookie(server.login("owner@example.com", PASSWORD).await);
    let desktop = Auth::Bearer(desktop_login(&server).await);
    let created = server
        .post("/api/v1/auth/tokens", json!({ "name": "script" }), &browser)
        .await;
    let script = Auth::Bearer(created.body["data"]["token"].as_str().unwrap().into());

    let response = server
        .post("/api/v1/auth/sessions/revoke-others", json!({}), &browser)
        .await;

    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.body["data"]["revoked"], 2);
    for (auth, expected) in [
        (&browser, StatusCode::OK),
        (&laptop, StatusCode::UNAUTHORIZED),
        (&desktop, StatusCode::UNAUTHORIZED),
        (&script, StatusCode::OK),
    ] {
        assert_eq!(server.get("/api/v1/auth/me", auth).await.status, expected);
    }
}

#[tokio::test]
async fn session_endpoints_need_a_login() {
    let server = server().await;
    server.setup_and_login().await;
    assert_eq!(
        server
            .get("/api/v1/auth/sessions", &Auth::None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}
