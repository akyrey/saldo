//! What the desktop app needs: a login that returns its token in the body,
//! and CORS for the app's own origin (`SALDO_ALLOWED_ORIGINS`).

#![allow(clippy::unwrap_used)] // the preflight helper is test code outside #[test]

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::*;
use serde_json::json;
use tower::ServiceExt;

const TAURI_ORIGIN: &str = "tauri://localhost";

#[tokio::test]
async fn a_desktop_login_returns_a_bearer_token_and_no_cookie() {
    let server = server().await;
    server.setup_and_login().await;

    let login = server
        .post(
            "/api/v1/auth/login",
            json!({ "email": "owner@example.com", "password": PASSWORD, "client": "desktop" }),
            &Auth::None,
        )
        .await;

    assert_eq!(login.status, StatusCode::OK, "{}", login.body);
    assert!(login.headers.get(header::SET_COOKIE).is_none());
    let token = login.body["data"]["token"].as_str().unwrap().to_string();
    let me = server
        .get("/api/v1/auth/me", &Auth::Bearer(token.clone()))
        .await;
    assert_eq!(me.status, StatusCode::OK);

    let logout = server
        .post(
            "/api/v1/auth/logout",
            json!({}),
            &Auth::Bearer(token.clone()),
        )
        .await;
    assert_eq!(logout.status, StatusCode::NO_CONTENT);
    let after = server.get("/api/v1/auth/me", &Auth::Bearer(token)).await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_browser_login_never_puts_the_token_in_the_body() {
    let server = server().await;
    server.setup_and_login().await;

    let login = server
        .post(
            "/api/v1/auth/login",
            json!({ "email": "owner@example.com", "password": PASSWORD }),
            &Auth::None,
        )
        .await;

    assert!(login.body["data"].get("token").is_none_or(|t| t.is_null()));
    assert!(session_cookie(&login.headers).is_some());
}

#[tokio::test]
async fn unknown_clients_are_rejected() {
    let server = server().await;
    server.setup_and_login().await;
    let login = server
        .post(
            "/api/v1/auth/login",
            json!({ "email": "owner@example.com", "password": PASSWORD, "client": "phone" }),
            &Auth::None,
        )
        .await;
    assert_eq!(login.status, StatusCode::UNPROCESSABLE_ENTITY);
}

/// A CORS preflight, as the desktop webview sends before a bearer request.
async fn preflight(server: &TestServer, origin: &str) -> axum::http::HeaderMap {
    let request = Request::builder()
        .method(Method::OPTIONS)
        .uri("/api/v1/accounts")
        .header(header::ORIGIN, origin)
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .header(
            header::ACCESS_CONTROL_REQUEST_HEADERS,
            "authorization,content-type",
        )
        .body(Body::empty())
        .unwrap();
    let response = server.router.clone().oneshot(request).await.unwrap();
    response.headers().clone()
}

#[tokio::test]
async fn cors_allows_only_the_configured_origins() {
    let server = server_with_env(&[(
        "SALDO_ALLOWED_ORIGINS",
        "tauri://localhost, http://tauri.localhost",
    )])
    .await;

    let allowed = preflight(&server, TAURI_ORIGIN).await;
    assert_eq!(allowed[header::ACCESS_CONTROL_ALLOW_ORIGIN], TAURI_ORIGIN);
    let methods = allowed[header::ACCESS_CONTROL_ALLOW_METHODS]
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        methods.contains("POST") && methods.contains("DELETE"),
        "{methods}"
    );
    assert!(
        allowed
            .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .is_none()
    );

    let other = preflight(&server, "https://evil.example").await;
    assert!(other.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
}

#[tokio::test]
async fn cors_is_off_without_configuration() {
    let server = server().await;
    let headers = preflight(&server, TAURI_ORIGIN).await;
    assert!(headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
}

#[test]
fn bad_origins_stop_startup() {
    let error = saldo_server::config::Config::from_lookup(|name| {
        (name == "SALDO_ALLOWED_ORIGINS").then(|| "not a url".to_string())
    })
    .unwrap_err();
    assert_eq!(error.variable, "SALDO_ALLOWED_ORIGINS");
}
