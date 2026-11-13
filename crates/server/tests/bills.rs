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
