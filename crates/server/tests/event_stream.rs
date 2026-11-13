#![allow(clippy::unwrap_used, clippy::expect_used)]

//! `GET /events/stream` (Server-Sent Events), read frame by frame.

mod common;

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use common::*;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

/// An open stream: the response body, read as text frames.
struct Stream {
    body: Body,
    buffer: String,
}

async fn open(server: &TestServer, token: &str, last_event_id: Option<&str>) -> Stream {
    let mut request = Request::get("/api/v1/events/stream")
        .header(header::COOKIE, format!("saldo_session={token}"));
    if let Some(id) = last_event_id {
        request = request.header("last-event-id", id);
    }
    let response = server
        .router
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/event-stream"
    );
    Stream {
        body: response.into_body(),
        buffer: String::new(),
    }
}

impl Stream {
    /// The next message's `(id, data)`, skipping keep-alive comments.
    async fn next(&mut self) -> (i64, Value) {
        loop {
            if let Some(end) = self.buffer.find("\n\n") {
                let message: String = self.buffer.drain(..end + 2).collect();
                let field = |name: &str| {
                    message
                        .lines()
                        .find_map(|line| line.strip_prefix(&format!("{name}: ")))
                        .map(str::to_string)
                };
                if let (Some(id), Some(data)) = (field("id"), field("data")) {
                    return (id.parse().unwrap(), serde_json::from_str(&data).unwrap());
                }
                continue;
            }
            let frame = tokio::time::timeout(Duration::from_secs(5), self.body.frame())
                .await
                .expect("a message within 5 s")
                .unwrap()
                .unwrap();
            if let Ok(data) = frame.into_data() {
                self.buffer.push_str(std::str::from_utf8(&data).unwrap());
            }
        }
    }

    /// True if nothing arrives for a short while.
    async fn is_quiet(&mut self) -> bool {
        tokio::time::timeout(Duration::from_millis(200), self.body.frame())
            .await
            .is_err()
    }
}

#[tokio::test]
async fn new_events_arrive_without_history() {
    let server = server().await;
    let token = server.setup_and_login().await;
    let mut stream = open(&server, &token, None).await;
    assert!(stream.is_quiet().await);

    let created = server
        .post(
            "/api/v1/tags",
            json!({ "name": "live" }),
            &Auth::Cookie(token.clone()),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let (id, data) = stream.next().await;
    assert_eq!(id, 5);
    assert_eq!(
        data,
        json!({ "seq": 5, "type": "tag.created", "entity_type": "tag",
                "entity_id": created.body["data"]["id"] })
    );
}

#[tokio::test]
async fn last_event_id_resumes_after_that_event() {
    let server = server().await;
    let token = server.setup_and_login().await;
    let mut stream = open(&server, &token, Some("2")).await;
    let types: Vec<(i64, Value)> = vec![stream.next().await, stream.next().await];
    assert_eq!(types[0].0, 3);
    assert_eq!(types[1].0, 4);
    assert_eq!(types[1].1["type"], "account.created");
}

#[tokio::test]
async fn shutdown_ends_open_streams() {
    let pool = saldo_app::db::connect_in_memory().await.unwrap();
    saldo_app::db::migrate(&pool).await.unwrap();
    let state = saldo_server::state::AppState::new(
        saldo_app::App::new(pool, std::sync::Arc::new(saldo_app::clock::SystemClock)),
        saldo_server::config::Config::from_lookup(|_| None).unwrap(),
    );
    let shutdown = state.shutdown.clone();
    let router = saldo_server::server::build_router(state);
    let setup = router
        .clone()
        .oneshot(
            Request::post("/api/v1/setup")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(setup_body("owner@example.com").to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let login = router
        .clone()
        .oneshot(
            Request::post("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "email": "owner@example.com", "password": PASSWORD }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let token = session_cookie(login.headers()).unwrap();
    let response = router
        .oneshot(
            Request::get("/api/v1/events/stream")
                .header(header::COOKIE, format!("saldo_session={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let mut body = response.into_body();
    shutdown.cancel();
    let end = tokio::time::timeout(Duration::from_secs(5), body.frame())
        .await
        .expect("the stream ends");
    assert!(end.is_none());
}
