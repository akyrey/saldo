//! `GET /api/v1/export/snapshot` on a real database file: off by default,
//! owner only, and a zip that `saldo restore` can read.

#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use chrono::{TimeZone, Utc};
use common::*;
use http_body_util::BodyExt;
use saldo_app::clock::FixedClock;
use saldo_app::{App, db};
use saldo_server::config::Config;
use saldo_server::server::build_router;
use saldo_server::state::AppState;
use serde_json::json;
use tower::ServiceExt;

/// Like `common::server_with_env`, but on `<temp dir>/saldo.db`, because
/// `VACUUM INTO` writes nothing for in-memory databases.
async fn file_server(extra: &[(&str, &str)]) -> (TestServer, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("saldo-snapshot-api-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let url = format!("sqlite://{}", dir.join("saldo.db").display());
    let pool = db::connect(&url).await.unwrap();
    db::migrate(&pool).await.unwrap();
    let clock = Arc::new(FixedClock::new(
        Utc.with_ymd_and_hms(2026, 11, 13, 9, 0, 0).unwrap(),
    ));
    let app = App::new(pool, clock.clone()).with_data_dir(dir.clone());
    let config = Config::from_lookup(|name| {
        extra
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.to_string())
    })
    .unwrap();
    let server = TestServer {
        router: build_router(AppState::new(app.clone(), config)),
        app,
        clock,
    };
    (server, dir)
}

/// The raw response: the common helper reads bodies as JSON or text.
async fn download(server: &TestServer, token: &str) -> (StatusCode, header::HeaderMap, Vec<u8>) {
    let request = Request::get("/api/v1/export/snapshot")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = server.router.clone().oneshot(request).await.unwrap();
    let (status, headers) = (response.status(), response.headers().clone());
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, headers, bytes.to_vec())
}

#[tokio::test]
async fn the_snapshot_is_off_unless_enabled() {
    let (server, _dir) = file_server(&[]).await;
    let token = server.setup_and_login().await;

    let (status, _, _) = download(&server, &token).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn owners_download_a_snapshot_members_get_403() {
    let (server, dir) = file_server(&[("SALDO_SNAPSHOT_DOWNLOAD", "true")]).await;
    let owner = server.setup_and_login().await;
    let added = server
        .post(
            "/api/v1/household/members",
            json!({ "email": "member@example.com", "display_name": "M", "password": PASSWORD }),
            &Auth::Cookie(owner.clone()),
        )
        .await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.body);
    let member = server.login("member@example.com", PASSWORD).await;

    let (status, headers, bytes) = download(&server, &owner).await;
    let (member_status, _, _) = download(&server, &member).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "application/zip");
    assert_eq!(
        headers[header::CONTENT_LENGTH],
        bytes.len().to_string().as_str()
    );
    let disposition = headers[header::CONTENT_DISPOSITION].to_str().unwrap();
    assert_eq!(
        disposition,
        "attachment; filename=\"saldo-snapshot-2026-11-13.zip\""
    );
    let archive = dir.join("downloaded.zip");
    std::fs::write(&archive, &bytes).unwrap();
    let target = dir.join("restored");
    let restored =
        saldo_app::snapshot::restore(&archive, &target.join("saldo.db"), &target).unwrap();
    assert_eq!(restored.files, 0);
    assert!(target.join("saldo.db").exists());
    assert_eq!(member_status, StatusCode::FORBIDDEN);
}
