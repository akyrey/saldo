#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::{Value, json};

async fn create(server: &TestServer, auth: &Auth, path: &str, body: Value) -> Value {
    let response = server.post(path, body, auth).await;
    assert_eq!(
        response.status,
        StatusCode::CREATED,
        "{path}: {}",
        response.body
    );
    response.body["data"].clone()
}

#[tokio::test]
async fn every_report_answers_with_its_range() {
    let server = server().await;
    let auth = Auth::Bearer(server.setup_and_login().await);
    let checking = create(
        &server,
        &auth,
        "/api/v1/accounts",
        json!({ "name": "Checking", "type": "asset" }),
    )
    .await;
    let shop = create(
        &server,
        &auth,
        "/api/v1/accounts",
        json!({ "name": "Shop", "type": "expense" }),
    )
    .await;
    create(
        &server,
        &auth,
        "/api/v1/transactions",
        json!({ "splits": [{
        "date": "2026-11-05", "description": "food", "source_id": checking["id"],
        "destination_id": shop["id"], "amount": "12.30"
    }] }),
    )
    .await;

    for report in [
        "net-worth",
        "cash-flow",
        "by-category",
        "by-tag",
        "budget-vs-actual",
    ] {
        let path = format!(
            "/api/v1/reports/{report}?from=2026-11-01&to=2026-11-30&interval=week&scope=mine"
        );
        let response = server.get(&path, &auth).await;
        assert_eq!(
            response.status,
            StatusCode::OK,
            "{report}: {}",
            response.body
        );
        assert_eq!(
            response.body["data"]["range"]["interval"], "week",
            "{report}"
        );
    }
    let flow = server
        .get(
            "/api/v1/reports/cash-flow?from=2026-11-01&to=2026-11-30",
            &auth,
        )
        .await;
    assert_eq!(flow.body["data"]["currencies"][0]["expenses"], "12.30");
    assert_eq!(flow.body["data"]["range"]["interval"], "month");

    for bad in [
        "/api/v1/reports/cash-flow?from=2026-11-01",
        "/api/v1/reports/cash-flow?from=2026-11-01&to=2026-11-30&interval=hour",
        "/api/v1/reports/cash-flow?from=2026-11-30&to=2026-11-01",
        "/api/v1/reports/cash-flow?from=2010-01-01&to=2026-11-01&interval=day",
        "/api/v1/reports/cash-flow?from=2026-11-01&to=2026-11-30&scope=nobody",
    ] {
        let status = server.get(bad, &auth).await.status;
        assert!(
            status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::BAD_REQUEST,
            "{bad}: {status}"
        );
    }
}
