#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use serde_json::{Value, json};

async fn created(server: &TestServer, auth: &Auth, path: &str, body: Value) -> Value {
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
async fn bills_and_periods_over_http() {
    let server = server().await;
    let owner = Auth::Cookie(server.setup_and_login().await);
    let checking = created(
        &server,
        &owner,
        "/api/v1/accounts",
        json!({ "name": "Checking", "type": "asset" }),
    )
    .await;
    let landlord = created(
        &server,
        &owner,
        "/api/v1/accounts",
        json!({ "name": "Landlord", "type": "expense" }),
    )
    .await;
    let bill = created(&server, &owner, "/api/v1/bills", json!({
        "name": "Rent", "amount_min": "800", "amount_max": "800", "first_due_date": "2026-10-02", "repeat_freq": "monthly"
    })).await;
    let id = bill["id"].as_str().unwrap();
    assert_eq!(bill["next_due_date"], "2026-10-02");
    assert_eq!(
        server.get("/api/v1/bills", &owner).await.body["meta"]["total"],
        1
    );

    // A payment saved with the bill pays the matching period.
    let payment = created(&server, &owner, "/api/v1/transactions", json!({ "splits": [{
        "date": "2026-10-03", "description": "Rent", "source_id": checking["id"], "destination_id": landlord["id"],
        "amount": "800", "bill_id": id
    }]})).await;
    let periods = server
        .get(&format!("/api/v1/bills/{id}/periods"), &owner)
        .await
        .body["data"]
        .clone();
    let october = periods
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["due_date"] == "2026-10-02")
        .unwrap();
    assert_eq!(
        (october["status"].as_str(), &october["paid_split_id"]),
        (Some("paid"), &payment["splits"][0]["id"])
    );
    let november = periods
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["due_date"] == "2026-11-02")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let skipped = server
        .send(
            Method::POST,
            &format!("/api/v1/bill-periods/{november}/skip"),
            None,
            &owner,
        )
        .await;
    assert_eq!(
        (skipped.status, skipped.body["data"]["status"].as_str()),
        (StatusCode::OK, Some("skipped"))
    );
    let reopened = server
        .send(
            Method::POST,
            &format!("/api/v1/bill-periods/{november}/reopen"),
            None,
            &owner,
        )
        .await;
    assert_eq!(reopened.body["data"]["status"], "open");
    let pay = server
        .post(
            &format!("/api/v1/bill-periods/{november}/pay"),
            json!({ "split_id": payment["splits"][0]["id"] }),
            &owner,
        )
        .await;
    assert_eq!(pay.body["data"]["status"], "paid");
    let twice = server
        .post(
            &format!("/api/v1/bill-periods/{november}/pay"),
            json!({ "split_id": payment["splits"][0]["id"] }),
            &owner,
        )
        .await;
    assert_eq!(twice.status, StatusCode::CONFLICT);

    let patched = server
        .send(
            Method::PATCH,
            &format!("/api/v1/bills/{id}"),
            Some(json!({ "amount_max": "850", "notes": "flat 2" })),
            &owner,
        )
        .await;
    assert_eq!(
        (
            patched.body["data"]["amount_max"].as_str(),
            patched.body["data"]["notes"].as_str()
        ),
        (Some("850.00"), Some("flat 2"))
    );
    let invalid = server.post("/api/v1/bills", json!({ "name": "X", "amount_min": "5", "amount_max": "1", "first_due_date": "2026-01-01", "repeat_freq": "monthly" }), &owner).await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);

    assert_eq!(
        server
            .send(Method::DELETE, &format!("/api/v1/bills/{id}"), None, &owner)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        server
            .get(&format!("/api/v1/bills/{id}"), &owner)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn rules_over_http_link_payments_to_bills() {
    let server = server().await;
    let owner = Auth::Cookie(server.setup_and_login().await);
    let checking = created(
        &server,
        &owner,
        "/api/v1/accounts",
        json!({ "name": "Checking", "type": "asset" }),
    )
    .await;
    let landlord = created(
        &server,
        &owner,
        "/api/v1/accounts",
        json!({ "name": "Landlord", "type": "expense" }),
    )
    .await;
    let bill = created(&server, &owner, "/api/v1/bills", json!({
        "name": "Rent", "amount_min": "800", "amount_max": "800", "first_due_date": "2026-10-02", "repeat_freq": "monthly"
    })).await;
    let group = created(
        &server,
        &owner,
        "/api/v1/rule-groups",
        json!({ "title": "Default" }),
    )
    .await;
    let rule = created(
        &server,
        &owner,
        "/api/v1/rules",
        json!({
            "rule_group_id": group["id"], "title": "Rent", "trigger_on": "create",
            "triggers": [{ "kind": "description_contains", "value": "rent" }],
            "actions": [{ "kind": "link_to_bill", "value": bill["id"] }]
        }),
    )
    .await;
    let rule_id = rule["id"].as_str().unwrap();

    let payment = created(&server, &owner, "/api/v1/transactions", json!({ "splits": [{
        "date": "2026-11-03", "description": "Rent November", "source_id": checking["id"], "destination_id": landlord["id"], "amount": "800"
    }]})).await;
    assert_eq!(payment["splits"][0]["bill_id"], bill["id"]);
    let periods = server
        .get(
            &format!("/api/v1/bills/{}/periods", bill["id"].as_str().unwrap()),
            &owner,
        )
        .await;
    assert!(
        periods.body["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["due_date"] == "2026-11-02" && p["status"] == "paid")
    );

    let preview = server
        .send(
            Method::POST,
            &format!("/api/v1/rules/{rule_id}/test"),
            None,
            &owner,
        )
        .await;
    assert_eq!(preview.body["data"]["total"], 1);
    let apply = server
        .send(
            Method::POST,
            &format!("/api/v1/rules/{rule_id}/apply"),
            None,
            &owner,
        )
        .await;
    assert_eq!(
        (
            apply.body["data"]["matched"].as_i64(),
            apply.body["data"]["changed"].as_i64()
        ),
        (Some(1), Some(0))
    );
    let listed = server.get("/api/v1/rules", &owner).await;
    assert_eq!(
        listed.body["data"][0]["triggers"][0]["kind"],
        "description_contains"
    );
    let bad = server.post("/api/v1/rules", json!({
        "rule_group_id": group["id"], "title": "Bad", "trigger_on": "create",
        "triggers": [{ "kind": "journal_id", "value": "1" }], "actions": [{ "kind": "clear_category" }]
    }), &owner).await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
    let replaced = server.send(Method::PUT, &format!("/api/v1/rules/{rule_id}"), Some(json!({
        "rule_group_id": group["id"], "title": "Rent v2", "trigger_on": "update", "active": false,
        "triggers": [{ "kind": "description_contains", "value": "rent" }], "actions": [{ "kind": "link_to_bill", "value": bill["id"] }]
    })), &owner).await;
    assert_eq!(
        (
            replaced.body["data"]["title"].as_str(),
            replaced.body["data"]["active"].as_bool()
        ),
        (Some("Rent v2"), Some(false))
    );
    assert_eq!(
        server
            .send(
                Method::DELETE,
                &format!("/api/v1/rule-groups/{}", group["id"].as_str().unwrap()),
                None,
                &owner
            )
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        server
            .get(&format!("/api/v1/rules/{rule_id}"), &owner)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn bill_history_over_http() {
    let server = server().await;
    let owner = Auth::Cookie(server.setup_and_login().await);
    let checking = created(
        &server,
        &owner,
        "/api/v1/accounts",
        json!({ "name": "Checking", "type": "asset" }),
    )
    .await;
    let gym = created(
        &server,
        &owner,
        "/api/v1/accounts",
        json!({ "name": "Gym", "type": "expense" }),
    )
    .await;
    let bill = created(&server, &owner, "/api/v1/bills", json!({
        "name": "Gym", "amount_min": "39", "amount_max": "45", "first_due_date": "2026-10-02", "repeat_freq": "monthly"
    })).await;
    let id = bill["id"].as_str().unwrap();
    created(&server, &owner, "/api/v1/transactions", json!({ "splits": [{
        "date": "2026-10-03", "description": "Gym", "source_id": checking["id"], "destination_id": gym["id"],
        "amount": "42.5", "bill_id": id
    }]})).await;

    let response = server
        .get(&format!("/api/v1/bills/{id}/history"), &owner)
        .await;

    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    let data = &response.body["data"];
    assert_eq!(data["bill_id"], id);
    assert_eq!(data["currency"], "EUR");
    assert_eq!(data["range"]["from"], "2026-10-02");
    assert_eq!(data["due_dates"][0]["status"], "paid");
    assert_eq!(data["payments"][0]["amount"], "42.50");
    assert_eq!(data["payments"][0]["destination"]["name"], "Gym");
    assert_eq!(data["payments"][0]["due_date"], "2026-10-02");
    assert_eq!(data["summary"]["total"], "42.50");
    assert_eq!(data["truncated"], false);

    let missing = "00000000-0000-0000-0000-000000000001";
    assert_eq!(
        server
            .get(&format!("/api/v1/bills/{missing}/history"), &owner)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        server
            .get(&format!("/api/v1/bills/{id}/history"), &Auth::None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}
