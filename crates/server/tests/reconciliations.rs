#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use serde_json::{Value, json};

async fn create(server: &TestServer, auth: &Auth, path: &str, body: Value) -> Response {
    server.post(path, body, auth).await
}

#[tokio::test]
async fn reconciles_an_account_over_the_api() {
    let server = server().await;
    let auth = Auth::Bearer(server.setup_and_login().await);
    let account = |name: &str, kind: &str| json!({ "name": name, "type": kind });
    let checking = create(
        &server,
        &auth,
        "/api/v1/accounts",
        account("Checking", "asset"),
    )
    .await;
    let shop = create(
        &server,
        &auth,
        "/api/v1/accounts",
        account("Shop", "expense"),
    )
    .await;
    let (checking, shop) = (&checking.body["data"]["id"], &shop.body["data"]["id"]);
    let spent = create(
        &server,
        &auth,
        "/api/v1/transactions",
        json!({ "splits": [{ "date": "2026-11-05", "description": "food", "source_id": checking,
                             "destination_id": shop, "amount": "12.30" }] }),
    )
    .await;
    let split = spent.body["data"]["splits"][0]["id"].clone();
    let base = format!(
        "/api/v1/accounts/{}/reconciliations",
        checking.as_str().unwrap()
    );

    let draft = server
        .get(&format!("{base}/draft?statement_date=2026-11-10"), &auth)
        .await;
    assert_eq!(draft.status, StatusCode::OK, "{}", draft.body);
    assert_eq!(draft.body["data"]["splits"][0]["amount"], "-12.30");

    let differs = json!({ "statement_date": "2026-11-10", "statement_balance": "-12.00",
                          "split_ids": [split] });
    let refused = create(&server, &auth, &base, differs.clone()).await;
    assert_eq!(
        refused.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        refused.body
    );
    let mut booked = differs;
    booked["book_difference"] = json!(true);
    let done = create(&server, &auth, &base, booked).await;
    assert_eq!(done.status, StatusCode::CREATED, "{}", done.body);
    assert_eq!(done.body["data"]["difference"], "0.30");

    let unreconciled = server
        .get("/api/v1/transactions?reconciled=false", &auth)
        .await;
    assert_eq!(unreconciled.body["meta"]["total"], 0);
    let list = server.get(&base, &auth).await;
    assert_eq!(list.body["meta"]["total"], 1);

    let path = format!(
        "/api/v1/reconciliations/{}",
        done.body["data"]["id"].as_str().unwrap()
    );
    let undone = server.send(Method::DELETE, &path, None, &auth).await;
    assert_eq!(undone.status, StatusCode::NO_CONTENT);
    let unreconciled = server
        .get("/api/v1/transactions?reconciled=false", &auth)
        .await;
    assert_eq!(unreconciled.body["meta"]["total"], 1);
}

/// The web editor does not send `reconciled`; saving must not clear it.
#[tokio::test]
async fn a_put_without_the_flag_keeps_a_split_reconciled() {
    let server = server().await;
    let auth = Auth::Bearer(server.setup_and_login().await);
    let account = |name: &str, kind: &str| json!({ "name": name, "type": kind });
    let checking = create(
        &server,
        &auth,
        "/api/v1/accounts",
        account("Checking", "asset"),
    )
    .await;
    let shop = create(
        &server,
        &auth,
        "/api/v1/accounts",
        account("Shop", "expense"),
    )
    .await;
    let (checking, shop) = (&checking.body["data"]["id"], &shop.body["data"]["id"]);
    let created = create(
        &server,
        &auth,
        "/api/v1/transactions",
        json!({ "splits": [{ "date": "2026-11-05", "description": "food", "source_id": checking,
                             "destination_id": shop, "amount": "12.30" }] }),
    )
    .await;
    let group = created.body["data"]["id"].as_str().unwrap().to_string();
    let split = created.body["data"]["splits"][0]["id"].clone();
    let base = format!(
        "/api/v1/accounts/{}/reconciliations",
        checking.as_str().unwrap()
    );
    let done = create(
        &server,
        &auth,
        &base,
        json!({ "statement_date": "2026-11-10", "statement_balance": "-12.30", "split_ids": [split] }),
    )
    .await;
    assert_eq!(done.status, StatusCode::CREATED, "{}", done.body);

    let path = format!("/api/v1/transactions/{group}");
    let edit = |description: &str, amount: &str| {
        json!({ "splits": [{ "id": split, "date": "2026-11-05", "description": description,
                             "source_id": checking, "destination_id": shop, "amount": amount }] })
    };
    let renamed = server
        .send(Method::PUT, &path, Some(edit("groceries", "12.30")), &auth)
        .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.body);
    assert_eq!(renamed.body["data"]["splits"][0]["reconciled"], true);

    let changed = server
        .send(Method::PUT, &path, Some(edit("groceries", "13.00")), &auth)
        .await;
    assert_eq!(changed.body["data"]["splits"][0]["reconciled"], false);
}
