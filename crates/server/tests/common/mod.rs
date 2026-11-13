//! Calls the real router in-process (no network) on a fresh in-memory database.

#![allow(dead_code)] // each test file uses a different subset
#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only helpers

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use chrono::{TimeZone, Utc};
use http_body_util::BodyExt;
use saldo_app::clock::FixedClock;
use saldo_app::{App, db};
use saldo_server::config::Config;
use saldo_server::server::build_router;
use saldo_server::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;

pub const ORIGIN: &str = "http://localhost:8080";
pub const PASSWORD: &str = "correct horse battery";

pub struct TestServer {
    pub router: Router,
    pub app: App,
    pub clock: Arc<FixedClock>,
}

/// How a test request authenticates.
#[derive(Clone)]
pub enum Auth {
    None,
    Bearer(String),
    /// Session cookie plus the CSRF header and a matching `Origin`.
    Cookie(String),
    /// Session cookie only, as a cross-site form would send it.
    CookieWithoutCsrf(String),
}

pub struct Response {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
}

pub async fn server() -> TestServer {
    server_with_env(&[]).await
}

/// A server whose configuration also has these `SALDO_*` variables set.
pub async fn server_with_env(extra: &[(&str, &str)]) -> TestServer {
    let pool = db::connect_in_memory().await.unwrap();
    db::migrate(&pool).await.unwrap();
    let clock = Arc::new(FixedClock::new(
        Utc.with_ymd_and_hms(2026, 11, 13, 9, 0, 0).unwrap(),
    ));
    let data_dir = std::env::temp_dir().join(format!("saldo-server-test-{}", uuid::Uuid::new_v4()));
    let app = App::new(pool, clock.clone()).with_data_dir(data_dir.clone());
    let config = Config::from_lookup(|name| {
        if name == "SALDO_DATA_DIR" {
            return Some(data_dir.display().to_string());
        }
        extra
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.to_string())
    })
    .unwrap();
    TestServer {
        router: build_router(AppState::new(app.clone(), config)),
        app,
        clock,
    }
}

impl TestServer {
    pub async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        auth: &Auth,
    ) -> Response {
        let mut request = Request::builder().method(method).uri(path);
        match auth {
            Auth::None => {}
            Auth::Bearer(token) => {
                request = request.header(header::AUTHORIZATION, format!("Bearer {token}"))
            }
            Auth::Cookie(token) => {
                request = request
                    .header(header::COOKIE, format!("theme=dark; saldo_session={token}"))
                    .header("x-saldo-csrf", "1")
                    .header(header::ORIGIN, ORIGIN)
            }
            Auth::CookieWithoutCsrf(token) => {
                request = request.header(header::COOKIE, format!("saldo_session={token}"))
            }
        }
        let body = match body {
            Some(value) => {
                request = request.header(header::CONTENT_TYPE, "application/json");
                Body::from(value.to_string())
            }
            None => Body::empty(),
        };
        let response = self
            .router
            .clone()
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into()))
        };
        Response {
            status,
            headers,
            body,
        }
    }

    /// A request with a raw body and content type (multipart uploads).
    pub async fn send_raw(
        &self,
        method: Method,
        path: &str,
        content_type: &str,
        body: Vec<u8>,
        auth: &Auth,
    ) -> Response {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::CONTENT_TYPE, content_type);
        if let Auth::Cookie(token) = auth {
            request = request
                .header(header::COOKIE, format!("saldo_session={token}"))
                .header("x-saldo-csrf", "1")
                .header(header::ORIGIN, ORIGIN);
        }
        let response = self
            .router
            .clone()
            .oneshot(request.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into()));
        Response {
            status,
            headers,
            body,
        }
    }

    pub async fn get(&self, path: &str, auth: &Auth) -> Response {
        self.send(Method::GET, path, None, auth).await
    }

    pub async fn post(&self, path: &str, body: Value, auth: &Auth) -> Response {
        self.send(Method::POST, path, Some(body), auth).await
    }

    /// Runs setup for `owner@example.com` and logs in; returns the session token.
    pub async fn setup_and_login(&self) -> String {
        let setup = self
            .post(
                "/api/v1/setup",
                setup_body("owner@example.com"),
                &Auth::None,
            )
            .await;
        assert_eq!(setup.status, StatusCode::CREATED, "{}", setup.body);
        self.login("owner@example.com", PASSWORD).await
    }

    pub async fn login(&self, email: &str, password: &str) -> String {
        let response = self
            .post(
                "/api/v1/auth/login",
                json!({ "email": email, "password": password }),
                &Auth::None,
            )
            .await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.body);
        session_cookie(&response.headers).expect("login sets the session cookie")
    }
}

pub fn setup_body(email: &str) -> Value {
    json!({
        "email": email,
        "password": PASSWORD,
        "display_name": "Owner",
        "household_name": "Home",
        "primary_currency": "EUR"
    })
}

/// The token in a `Set-Cookie: saldo_session=...` header, if any.
pub fn session_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|v| v.strip_prefix("saldo_session="))
        .map(|rest| rest.split(';').next().unwrap_or_default().to_string())
}
