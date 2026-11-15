#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use axum::http::{Method, StatusCode, header};
use common::*;
use serde_json::json;

const BOUNDARY: &str = "saldo-test-boundary";

/// A multipart/form-data body with text fields and one file part.
fn form(fields: &[(&str, &str)], file: Option<(&str, &str, &[u8])>) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .bytes(),
        );
    }
    if let Some((filename, mime, bytes)) = file {
        body.extend(format!("--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {mime}\r\n\r\n").bytes());
        body.extend_from_slice(bytes);
        body.extend(b"\r\n");
    }
    body.extend(format!("--{BOUNDARY}--\r\n").bytes());
    body
}

#[tokio::test]
async fn upload_read_download_delete_over_http() {
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
    let content_type = format!("multipart/form-data; boundary={BOUNDARY}");

    let body = form(
        &[
            ("attachable_type", "account"),
            ("attachable_id", &account_id),
            ("title", "Contract"),
        ],
        Some(("contract.txt", "text/plain", b"signed")),
    );
    let created = server
        .send_raw(
            Method::POST,
            "/api/v1/attachments",
            &content_type,
            body,
            &owner,
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(created.body["data"]["filename"], "contract.txt");
    assert_eq!(created.body["data"]["size_bytes"], 6);
    let id = created.body["data"]["id"].as_str().unwrap().to_string();

    let listed = server
        .get(
            &format!("/api/v1/attachments?attachable_type=account&attachable_id={account_id}"),
            &owner,
        )
        .await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.body);
    assert_eq!(listed.body["data"][0]["id"], id.as_str());
    assert_eq!(listed.body["data"].as_array().unwrap().len(), 1);
    let unknown_type = server
        .get(
            &format!("/api/v1/attachments?attachable_type=budget&attachable_id={account_id}"),
            &owner,
        )
        .await;
    assert_eq!(unknown_type.status, StatusCode::UNPROCESSABLE_ENTITY);

    let meta = server
        .get(&format!("/api/v1/attachments/{id}"), &owner)
        .await;
    assert_eq!(meta.body["data"]["title"], "Contract");
    let download = server
        .get(&format!("/api/v1/attachments/{id}/download"), &owner)
        .await;
    assert_eq!(download.status, StatusCode::OK);
    assert_eq!(download.body, "signed");
    assert_eq!(
        download.headers[header::CONTENT_DISPOSITION],
        "attachment; filename=\"contract.txt\""
    );
    assert_eq!(download.headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert_eq!(download.headers[header::CONTENT_TYPE], "text/plain");

    let deleted = server
        .send(
            Method::DELETE,
            &format!("/api/v1/attachments/{id}"),
            None,
            &owner,
        )
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    assert_eq!(
        server
            .get(&format!("/api/v1/attachments/{id}"), &owner)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn bad_uploads_are_rejected() {
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
    let content_type = format!("multipart/form-data; boundary={BOUNDARY}");
    let target = [
        ("attachable_type", "account"),
        ("attachable_id", account_id.as_str()),
    ];

    let html = server
        .send_raw(
            Method::POST,
            "/api/v1/attachments",
            &content_type,
            form(&target, Some(("x.html", "text/html", b"<script>"))),
            &owner,
        )
        .await;
    assert_eq!(html.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(html.body["error"]["details"][0]["field"], "mime_type");
    let missing = server
        .send_raw(
            Method::POST,
            "/api/v1/attachments",
            &content_type,
            form(&target, None),
            &owner,
        )
        .await;
    assert_eq!(missing.status, StatusCode::UNPROCESSABLE_ENTITY);
    let garbage = server
        .send_raw(
            Method::POST,
            "/api/v1/attachments",
            "multipart/form-data",
            b"nope".to_vec(),
            &owner,
        )
        .await;
    assert_eq!(garbage.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn split_attachments_follow_their_split_over_http() {
    let server = server().await;
    let owner = Auth::Cookie(server.setup_and_login().await);
    let mut ids = Vec::new();
    for (name, kind) in [("Checking", "asset"), ("Food", "expense")] {
        let created = server
            .post(
                "/api/v1/accounts",
                json!({ "name": name, "type": kind }),
                &owner,
            )
            .await;
        ids.push(created.body["data"]["id"].as_str().unwrap().to_string());
    }
    let split = |id: Option<&str>, amount: &str| {
        json!({ "id": id, "date": "2026-11-01", "description": "Lunch",
                "source_id": ids[0], "destination_id": ids[1], "amount": amount })
    };
    let created = server
        .post(
            "/api/v1/transactions",
            json!({ "splits": [split(None, "5")] }),
            &owner,
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let group_id = created.body["data"]["id"].as_str().unwrap().to_string();
    let split_id = created.body["data"]["splits"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let content_type = format!("multipart/form-data; boundary={BOUNDARY}");
    let body = form(
        &[("attachable_type", "split"), ("attachable_id", &split_id)],
        Some(("lunch.txt", "text/plain", b"receipt")),
    );
    let uploaded = server
        .send_raw(
            Method::POST,
            "/api/v1/attachments",
            &content_type,
            body,
            &owner,
        )
        .await;
    assert_eq!(uploaded.status, StatusCode::CREATED, "{}", uploaded.body);
    let listing = format!("/api/v1/attachments?attachable_type=split&attachable_id={split_id}");

    let put = server
        .send(
            Method::PUT,
            &format!("/api/v1/transactions/{group_id}"),
            Some(json!({ "splits": [split(Some(&split_id), "6")] })),
            &owner,
        )
        .await;
    assert_eq!(put.status, StatusCode::OK, "{}", put.body);
    assert_eq!(put.body["data"]["splits"][0]["id"], split_id.as_str());
    let kept = server.get(&listing, &owner).await;
    assert_eq!(kept.body["data"].as_array().unwrap().len(), 1);

    let deleted = server
        .send(
            Method::DELETE,
            &format!("/api/v1/transactions/{group_id}"),
            None,
            &owner,
        )
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let gone = server.get(&listing, &owner).await;
    assert!(gone.body["data"].as_array().unwrap().is_empty());
}
