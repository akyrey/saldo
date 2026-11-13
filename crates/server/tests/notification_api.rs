//! `/notification-channels` and `/push/vapid-key` through the router.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use axum::http::{Method, StatusCode};
use chrono::{TimeZone, Utc};
use common::{Auth, TestServer};
use saldo_app::clock::FixedClock;
use saldo_app::{App, db};
use saldo_server::config::Config;
use saldo_server::server::build_router;
use saldo_server::state::AppState;
use serde_json::json;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const VAPID_PRIVATE: &str = "RS0WdYWWo1HajXg3NZR1olzCf31i-ZBGDkFyCs7j1jw";
const P256DH: &str =
    "BLn9b-VR0ca83knDNZ32dCHGyjJp-1riX9ZTN40MqV8K_LpQmLqxC_DoHvqvFXO_nGdAB4W9dogZb_sM-uV4JbY";
const AUTH: &str = "_ordMnz7uTCmrpBTeUV4Bw";

/// A server whose configuration enables Web Push.
async fn server_with_vapid() -> TestServer {
    let pool = db::connect_in_memory().await.unwrap();
    db::migrate(&pool).await.unwrap();
    let clock = Arc::new(FixedClock::new(
        Utc.with_ymd_and_hms(2026, 11, 13, 9, 0, 0).unwrap(),
    ));
    let app = App::new(pool, clock.clone());
    let config = Config::from_lookup(|name| match name {
        "SALDO_VAPID_PRIVATE_KEY" => Some(VAPID_PRIVATE.into()),
        "SALDO_VAPID_SUBJECT" => Some("mailto:me@example.com".into()),
        _ => None,
    })
    .unwrap();
    TestServer {
        router: build_router(AppState::new(app.clone(), config)),
        app,
        clock,
    }
}

#[tokio::test]
async fn channels_are_created_tested_changed_and_deleted_without_echoing_secrets() {
    let server = common::server().await;
    let auth = Auth::Cookie(server.setup_and_login().await);
    let ntfy = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&ntfy)
        .await;

    let created = server
        .post(
            "/api/v1/notification-channels",
            json!({ "kind": "ntfy", "name": "Phone",
                    "config": { "server_url": ntfy.uri(), "topic": "bills", "token": "tk_secret" } }),
            &auth,
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert!(!created.body.to_string().contains("tk_secret"));
    assert_eq!(created.body["data"]["settings"]["has_token"], true);
    let id = created.body["data"]["id"].as_str().unwrap().to_string();

    let tested = server
        .post(
            &format!("/api/v1/notification-channels/{id}/test"),
            json!({}),
            &auth,
        )
        .await;
    assert_eq!(tested.body["data"], json!({ "sent": true, "error": null }));

    let patched = server
        .send(
            Method::PATCH,
            &format!("/api/v1/notification-channels/{id}"),
            Some(json!({ "active": false, "config": { "topic": "other" } })),
            &auth,
        )
        .await;
    assert_eq!(patched.status, StatusCode::OK, "{}", patched.body);
    assert_eq!(patched.body["data"]["active"], false);
    assert_eq!(patched.body["data"]["settings"]["topic"], "other");

    let list = server.get("/api/v1/notification-channels", &auth).await;
    assert_eq!(list.body["data"].as_array().unwrap().len(), 1);
    let deleted = server
        .send(
            Method::DELETE,
            &format!("/api/v1/notification-channels/{id}"),
            None,
            &auth,
        )
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let gone = server
        .post(
            &format!("/api/v1/notification-channels/{id}/test"),
            json!({}),
            &auth,
        )
        .await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_failed_test_send_reports_why() {
    let server = common::server().await;
    let auth = Auth::Cookie(server.setup_and_login().await);
    let gotify = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&gotify)
        .await;
    let created = server
        .post(
            "/api/v1/notification-channels",
            json!({ "kind": "gotify", "name": "Gotify",
                    "config": { "server_url": gotify.uri(), "token": "bad" } }),
            &auth,
        )
        .await;
    let id = created.body["data"]["id"].as_str().unwrap();
    let tested = server
        .post(
            &format!("/api/v1/notification-channels/{id}/test"),
            json!({}),
            &auth,
        )
        .await;
    assert_eq!(tested.body["data"]["sent"], false);
    assert_eq!(tested.body["data"]["error"], "the server answered HTTP 401");
}

#[tokio::test]
async fn kinds_without_server_settings_and_bad_configs_are_rejected() {
    let server = common::server().await;
    let auth = Auth::Cookie(server.setup_and_login().await);
    let cases = [
        json!({ "kind": "email", "name": "Mail", "config": { "to": "me@example.com" } }),
        json!({ "kind": "webpush", "name": "Browser", "config": {} }),
        json!({ "kind": "ntfy", "name": "Phone", "config": { "server_url": "nope", "topic": "x" } }),
        json!({ "kind": "sms", "name": "SMS", "config": {} }),
    ];
    for body in cases {
        let response = server
            .post("/api/v1/notification-channels", body.clone(), &auth)
            .await;
        assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    }
    let key = server.get("/api/v1/push/vapid-key", &auth).await;
    assert_eq!(key.body["data"]["public_key"], json!(null));
}

#[tokio::test]
async fn with_vapid_the_key_is_served_and_browsers_can_subscribe() {
    let server = server_with_vapid().await;
    let auth = Auth::Cookie(server.setup_and_login().await);
    let key = server.get("/api/v1/push/vapid-key", &auth).await;
    let public_key = key.body["data"]["public_key"].as_str().unwrap();
    assert_eq!(public_key.len(), 87);
    assert!(!key.body.to_string().contains(VAPID_PRIVATE));

    let subscription = json!({
        "endpoint": "https://push.example.com/send/secret-id",
        "expirationTime": null,
        "keys": { "p256dh": P256DH, "auth": AUTH },
    });
    let created = server
        .post(
            "/api/v1/notification-channels",
            json!({ "kind": "webpush", "name": "This browser", "config": subscription }),
            &auth,
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let shown = created.body.to_string();
    for secret in ["secret-id", P256DH, AUTH] {
        assert!(!shown.contains(secret), "{shown}");
    }
    assert_eq!(
        created.body["data"]["settings"]["push_service"],
        "https://push.example.com"
    );
}

#[tokio::test]
async fn members_only_see_their_own_channels() {
    let server = common::server().await;
    let owner = Auth::Cookie(server.setup_and_login().await);
    let added = server
        .post(
            "/api/v1/household/members",
            json!({ "email": "partner@example.com", "display_name": "Partner", "password": common::PASSWORD }),
            &owner,
        )
        .await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.body);
    let partner = Auth::Cookie(server.login("partner@example.com", common::PASSWORD).await);
    let created = server
        .post(
            "/api/v1/notification-channels",
            json!({ "kind": "ntfy", "name": "Owner phone",
                    "config": { "server_url": "https://ntfy.sh", "topic": "x" } }),
            &owner,
        )
        .await;
    let id = created.body["data"]["id"].as_str().unwrap();
    let list = server.get("/api/v1/notification-channels", &partner).await;
    assert_eq!(list.body["data"], json!([]));
    let delete = server
        .send(
            Method::DELETE,
            &format!("/api/v1/notification-channels/{id}"),
            None,
            &partner,
        )
        .await;
    assert_eq!(delete.status, StatusCode::NOT_FOUND);
}
