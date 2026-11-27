#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

//! Statement import over HTTP (Phase 13): profiles, preview, upload,
//! review, commit, revert.

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use serde_json::{Value, json};

const BOUNDARY: &str = "saldo-statement-boundary";

fn form(fields: &[(&str, &str)], file: Option<(&str, &[u8])>) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .bytes(),
        );
    }
    if let Some((filename, bytes)) = file {
        body.extend(format!("--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: text/csv\r\n\r\n").bytes());
        body.extend_from_slice(bytes);
        body.extend(b"\r\n");
    }
    body.extend(format!("--{BOUNDARY}--\r\n").bytes());
    body
}

fn multipart() -> String {
    format!("multipart/form-data; boundary={BOUNDARY}")
}

const CSV: &[u8] =
    b"Data;Descrizione;Importo\n05/11/2026;Coffee;-1,50\n06/11/2026;Lunch;-12,00\nTotale;;-13,50\n";

/// The Mediobanca preset reshaped for the CSV above.
fn csv_mapping(presets: &Value) -> Value {
    let mut mapping = presets["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "Mediobanca")
        .unwrap()["mapping"]
        .clone();
    mapping["date_column"] = json!(["Data"]);
    mapping["value_date_column"] = Value::Null;
    mapping["amount"] = json!({ "kind": "signed", "column": ["Importo"] });
    mapping["description_columns"] = json!([["Descrizione"]]);
    mapping["currency_column"] = Value::Null;
    mapping
}

#[tokio::test]
async fn a_statement_goes_from_preview_to_commit_and_back_over_http() {
    let server = server().await;
    let owner = Auth::Cookie(server.setup_and_login().await);
    let account = server
        .post(
            "/api/v1/accounts",
            json!({ "name": "Checking", "type": "asset" }),
            &owner,
        )
        .await;
    let account_id = account.body["data"]["id"].as_str().unwrap().to_string();

    let presets = server
        .get("/api/v1/statement-profiles/presets", &owner)
        .await;
    assert_eq!(presets.status, StatusCode::OK);
    assert_eq!(presets.body["data"].as_array().unwrap().len(), 3);
    let mapping = csv_mapping(&presets.body);

    // Preview with the unsaved mapping.
    let mapping_text = mapping.to_string();
    let preview = server
        .send_raw(
            Method::POST,
            "/api/v1/statement-profiles/preview",
            &multipart(),
            form(
                &[
                    ("format", "csv"),
                    ("mapping", &mapping_text),
                    ("account_id", &account_id),
                ],
                Some(("nov.csv", CSV)),
            ),
            &owner,
        )
        .await;
    assert_eq!(preview.status, StatusCode::OK, "{:?}", preview.body);
    assert_eq!(preview.body["data"]["total_lines"], 2);
    assert_eq!(preview.body["data"]["unsupported"][0]["row"], 4);

    let profile = server
        .post(
            "/api/v1/statement-profiles",
            json!({ "name": "Bank", "account_id": account_id, "format": "csv", "mapping": mapping }),
            &owner,
        )
        .await;
    assert_eq!(profile.status, StatusCode::CREATED, "{:?}", profile.body);
    let profile_id = profile.body["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        server.get("/api/v1/statement-profiles", &owner).await.body["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let upload = server
        .send_raw(
            Method::POST,
            "/api/v1/statement-imports",
            &multipart(),
            form(&[("profile_id", &profile_id)], Some(("nov.csv", CSV))),
            &owner,
        )
        .await;
    assert_eq!(upload.status, StatusCode::CREATED, "{:?}", upload.body);
    let import_id = upload.body["data"]["import"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(upload.body["data"]["lines"].as_array().unwrap().len(), 2);
    assert_eq!(upload.body["data"]["import"]["filename"], "nov.csv");

    let skip = server
        .send(
            Method::PATCH,
            &format!("/api/v1/statement-imports/{import_id}/lines/1"),
            Some(json!({ "action": "skip" })),
            &owner,
        )
        .await;
    assert_eq!(skip.status, StatusCode::OK, "{:?}", skip.body);
    assert_eq!(skip.body["data"]["action"], "skip");

    let committed = server
        .post(
            &format!("/api/v1/statement-imports/{import_id}/commit"),
            json!({}),
            &owner,
        )
        .await;
    assert_eq!(committed.status, StatusCode::OK, "{:?}", committed.body);
    assert_eq!(committed.body["data"]["import"]["status"], "committed");
    let transactions = server
        .get(
            &format!("/api/v1/transactions?account_id={account_id}"),
            &owner,
        )
        .await;
    assert_eq!(transactions.body["data"].as_array().unwrap().len(), 1);

    let listed = server
        .get(
            &format!("/api/v1/statement-imports?account_id={account_id}"),
            &owner,
        )
        .await;
    assert_eq!(listed.body["meta"]["total"], 1);
    let detail = server
        .get(&format!("/api/v1/statement-imports/{import_id}"), &owner)
        .await;
    assert_eq!(detail.body["data"]["lines"][0]["status"], "new");

    let reverted = server
        .post(
            &format!("/api/v1/statement-imports/{import_id}/revert"),
            json!({}),
            &owner,
        )
        .await;
    assert_eq!(reverted.status, StatusCode::OK);
    assert_eq!(reverted.body["data"]["status"], "reverted");
    let after = server
        .get(
            &format!("/api/v1/transactions?account_id={account_id}"),
            &owner,
        )
        .await;
    assert!(after.body["data"].as_array().unwrap().is_empty());
    let again = server
        .post(
            &format!("/api/v1/statement-imports/{import_id}/revert"),
            json!({}),
            &owner,
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn bad_requests_get_the_error_envelope() {
    let server = server().await;
    let owner = Auth::Cookie(server.setup_and_login().await);
    let bad_mapping = server
        .post(
            "/api/v1/statement-profiles",
            json!({ "name": "x", "format": "csv", "mapping": { "nope": 1 } }),
            &owner,
        )
        .await;
    assert_eq!(bad_mapping.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        bad_mapping.body["error"]["details"][0]["field"], "mapping",
        "{:?}",
        bad_mapping.body
    );

    let no_file = server
        .send_raw(
            Method::POST,
            "/api/v1/statement-imports",
            &multipart(),
            form(&[("profile_id", "x")], None),
            &owner,
        )
        .await;
    assert_eq!(
        no_file.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{:?}",
        no_file.body
    );

    let missing = server
        .get(
            "/api/v1/statement-imports/00000000-0000-0000-0000-000000000001",
            &owner,
        )
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}
