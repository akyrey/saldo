mod common;

use axum::http::StatusCode;
use common::*;

#[tokio::test]
async fn healthz_reports_ok_and_carries_a_request_id() {
    let server = server().await;
    let response = server.get("/healthz", &Auth::None).await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["status"], "ok");
    assert_eq!(response.body["app"], "saldo");
    assert_eq!(response.body["api"], "v1");
    assert!(
        response.body["version"]
            .as_str()
            .is_some_and(|v| !v.is_empty())
    );
    assert!(response.headers.contains_key("x-request-id"));
}

#[tokio::test]
async fn healthz_reports_unavailable_when_the_database_is_gone() {
    let server = server().await;
    server.app.pool.close().await;
    let response = server.get("/healthz", &Auth::None).await;
    assert_eq!(response.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.body["status"], "unavailable");
}

#[tokio::test]
async fn openapi_document_is_served_without_authentication() {
    let server = server().await;
    let response = server.get("/api/v1/openapi.json", &Auth::None).await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["info"]["title"], "Saldo API");
    assert!(response.body["paths"]["/healthz"].is_object());
}

#[tokio::test]
async fn unknown_paths_are_not_found() {
    let server = server().await;
    assert_eq!(
        server.get("/api/v1/nope", &Auth::None).await.status,
        StatusCode::NOT_FOUND
    );
}
