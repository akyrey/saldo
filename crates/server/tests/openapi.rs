#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use serde_json::Value;

const COMMITTED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/openapi.json");

/// Routes that work without a session or token.
const PUBLIC: [(&str, &str); 7] = [
    ("get", "/healthz"),
    ("get", "/api/v1/setup"),
    ("post", "/api/v1/setup"),
    ("post", "/api/v1/auth/login"),
    // "Log in with Authentik" happens before there is a session.
    ("get", "/api/v1/auth/methods"),
    ("get", "/api/v1/auth/oidc/start"),
    ("get", "/api/v1/auth/oidc/callback"),
];

fn document() -> Value {
    serde_json::from_str(&saldo_server::openapi::document_json().unwrap()).unwrap()
}

/// Every (method, path) pair in the document.
fn operations(document: &Value) -> Vec<(String, String)> {
    let paths = document["paths"].as_object().unwrap();
    paths
        .iter()
        .flat_map(|(path, item)| {
            item.as_object()
                .unwrap()
                .keys()
                .map(move |method| (method.clone(), path.clone()))
        })
        .collect()
}

#[test]
fn committed_document_is_up_to_date() {
    let generated = saldo_server::openapi::document_json().unwrap();
    let committed = std::fs::read_to_string(COMMITTED).unwrap_or_default();
    assert!(
        committed.trim_end() == generated.trim_end(),
        "web/openapi.json is stale; run: cargo run -p saldo-server -- openapi > web/openapi.json"
    );
}

#[test]
fn every_protected_operation_declares_its_security() {
    let document = document();
    for (method, path) in operations(&document) {
        let declared = document["paths"][&path][&method]["security"]
            .as_array()
            .is_some_and(|s| !s.is_empty());
        let public = PUBLIC.contains(&(method.as_str(), path.as_str()));
        assert_eq!(declared, !public, "{method} {path}");
    }
}

#[tokio::test]
async fn only_public_routes_work_without_authentication() {
    let server = server().await;
    server.setup_and_login().await;
    let document = document();
    let some_id = "0190f5c3-7a2b-7c00-8000-000000000001";
    let mut checked = 0;
    for (method, path) in operations(&document) {
        if PUBLIC.contains(&(method.as_str(), path.as_str())) {
            continue;
        }
        let url = path.replace("{id}", some_id).replace("{user_id}", some_id);
        let method = Method::from_bytes(method.to_uppercase().as_bytes()).unwrap();
        let response = server.send(method.clone(), &url, None, &Auth::None).await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED, "{method} {url}");
        checked += 1;
    }
    assert!(checked >= 30, "only {checked} operations checked");
}
