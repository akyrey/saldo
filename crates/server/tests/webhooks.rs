//! `/webhooks` and `/webhook-deliveries` over HTTP.

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use serde_json::json;

async fn owner_and_member(server: &TestServer) -> (Auth, Auth) {
    let owner = Auth::Cookie(server.setup_and_login().await);
    let member =
        json!({ "email": "member@example.com", "display_name": "M", "password": PASSWORD });
    let added = server
        .post("/api/v1/household/members", member, &owner)
        .await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.body);
    let member = Auth::Cookie(server.login("member@example.com", PASSWORD).await);
    (owner, member)
}

#[tokio::test]
async fn owners_manage_endpoints_and_see_the_secret_once() {
    let server = server().await;
    let (owner, member) = owner_and_member(&server).await;
    let body = json!({ "name": "Home Assistant", "url": "http://192.168.1.5:8123/api/webhook/x",
                       "event_types": ["transaction.*"] });
    let created = server.post("/api/v1/webhooks", body.clone(), &owner).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let secret = created.body["data"]["secret"].as_str().unwrap().to_string();
    assert!(secret.starts_with("whsec_"));
    let id = created.body["data"]["webhook"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let path = format!("/api/v1/webhooks/{id}");

    let listed = server.get("/api/v1/webhooks", &owner).await;
    assert_eq!(listed.status, StatusCode::OK);
    assert_eq!(
        listed.body["data"][0]["event_types"],
        json!(["transaction.*"])
    );
    assert!(!listed.body.to_string().contains(&secret));
    assert!(
        !server
            .get(&path, &owner)
            .await
            .body
            .to_string()
            .contains(&secret)
    );

    for (method, uri) in [
        (Method::GET, "/api/v1/webhooks".to_string()),
        (Method::POST, "/api/v1/webhooks".to_string()),
        (Method::GET, path.clone()),
        (Method::DELETE, path.clone()),
        (Method::POST, format!("{path}/test")),
    ] {
        let body = (method == Method::POST).then(|| body.clone());
        let denied = server.send(method, &uri, body, &member).await;
        assert_eq!(denied.status, StatusCode::FORBIDDEN, "{uri}");
    }

    let patched = server
        .send(
            Method::PATCH,
            &path,
            Some(json!({ "active": false, "name": "HA" })),
            &owner,
        )
        .await;
    assert_eq!(patched.status, StatusCode::OK, "{}", patched.body);
    assert_eq!(
        (
            patched.body["data"]["active"].clone(),
            patched.body["data"]["name"].clone()
        ),
        (json!(false), json!("HA"))
    );

    let rotated = server
        .post(&format!("{path}/rotate-secret"), json!({}), &owner)
        .await;
    assert_eq!(rotated.status, StatusCode::OK);
    assert_ne!(rotated.body["data"]["secret"].as_str().unwrap(), secret);

    let deleted = server.send(Method::DELETE, &path, None, &owner).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    assert_eq!(
        server.get(&path, &owner).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn invalid_input_is_a_422_on_the_field() {
    let server = server().await;
    let owner = Auth::Cookie(server.setup_and_login().await);
    let bad = server
        .post(
            "/api/v1/webhooks",
            json!({ "name": "x", "url": "ftp://example.com" }),
            &owner,
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(bad.body["error"]["details"][0]["field"], "url");
}

#[tokio::test]
async fn test_queues_a_delivery_that_can_be_redelivered() {
    let server = server().await;
    let owner = Auth::Cookie(server.setup_and_login().await);
    server.clock.advance(chrono::TimeDelta::seconds(1));
    let body =
        json!({ "name": "Catcher", "url": "http://127.0.0.1:9/hook", "event_types": ["bill.*"] });
    let created = server.post("/api/v1/webhooks", body, &owner).await;
    let id = created.body["data"]["webhook"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let queued = server
        .post(&format!("/api/v1/webhooks/{id}/test"), json!({}), &owner)
        .await;
    assert_eq!(queued.status, StatusCode::ACCEPTED, "{}", queued.body);
    let event_id = queued.body["data"]["event_id"]
        .as_str()
        .unwrap()
        .to_string();
    server.app.webhooks.dispatch().await.unwrap();

    let list = server
        .get(&format!("/api/v1/webhooks/{id}/deliveries"), &owner)
        .await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.body);
    assert_eq!(list.body["meta"]["total"], 1);
    let delivery = &list.body["data"][0];
    assert_eq!(delivery["event_id"], event_id.as_str());
    assert_eq!(delivery["event_type"], "webhook.test");
    assert_eq!(delivery["status"], "pending");

    let delivery_id = delivery["id"].as_str().unwrap();
    let again = server
        .post(
            &format!("/api/v1/webhook-deliveries/{delivery_id}/redeliver"),
            json!({}),
            &owner,
        )
        .await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.body);
    assert_eq!(again.body["data"]["attempts"], 0);
    let missing = server
        .post(
            &format!(
                "/api/v1/webhook-deliveries/{}/redeliver",
                uuid::Uuid::new_v4()
            ),
            json!({}),
            &owner,
        )
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}
