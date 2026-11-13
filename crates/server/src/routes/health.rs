//! `GET /healthz`: is the server up and the database reachable?

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Serialize;
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct Health {
    /// `ok` or `unavailable`.
    pub status: String,
    /// Always `saldo`: lets the desktop app check it found a Saldo server.
    pub app: String,
    /// The API version clients speak (`v1`).
    pub api: String,
    /// The server's version, for example `0.1.0`.
    pub version: String,
}

impl Health {
    fn new(status: &str) -> Self {
        Self {
            status: status.into(),
            app: "saldo".into(),
            api: "v1".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }
}

/// Health check (no authentication).
#[utoipa::path(get, path = "/healthz", tag = "health",
    responses((status = 200, body = Health), (status = 503, body = Health)))]
pub async fn healthz(State(state): State<AppState>) -> (StatusCode, Json<Health>) {
    match sqlx_ping(&state).await {
        true => (StatusCode::OK, Json(Health::new("ok"))),
        false => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(Health::new("unavailable")),
        ),
    }
}

async fn sqlx_ping(state: &AppState) -> bool {
    match state.app.ping().await {
        Ok(()) => true,
        Err(error) => {
            tracing::error!(%error, "health check failed");
            false
        }
    }
}
