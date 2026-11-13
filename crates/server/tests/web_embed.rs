//! The embedded web app, only with `--features embed-web` (needs `web/dist`
//! from `npm run build`).
#![cfg(feature = "embed-web")]

mod common;

use axum::http::{StatusCode, header};
use common::*;

#[tokio::test]
async fn serves_the_app_for_pages_and_json_for_unknown_api_paths() {
    let server = server().await;
    let page = server.get("/transactions/new", &Auth::None).await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(
        page.body
            .as_str()
            .unwrap_or("")
            .contains("<div id=\"root\">")
    );
    assert_eq!(page.headers[header::CACHE_CONTROL], "no-cache");

    let api = server.get("/api/v1/nope", &Auth::None).await;
    assert_eq!(api.status, StatusCode::NOT_FOUND);
    assert_eq!(api.body["error"]["code"], "not_found");

    let health = server.get("/healthz", &Auth::None).await;
    assert_eq!(health.status, StatusCode::OK, "real routes still win");
}
