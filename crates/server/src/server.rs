//! Building the router and running the server.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::http::{HeaderName, Request};
use axum::routing::get;
use axum::{Json, Router};
use saldo_app::clock::SystemClock;
use saldo_app::{App, db};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::{Level, info};

use crate::config::Config;
use crate::openapi;
use crate::state::AppState;

const REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

/// The complete application: documented routes, the OpenAPI document, and
/// the logging layers. Tests call this router directly, without a network.
pub fn build_router(state: AppState) -> Router {
    let (router, document) = openapi::router().split_for_parts();
    let document = Arc::new(document);
    let router = router.route(
        "/api/v1/openapi.json",
        get(move || async move { Json(document.as_ref().clone()) }),
    );
    crate::spa::with_web_app(router)
        // Layers wrap every request, like Laravel middleware. The last one
        // added runs first: set a request ID, log, then copy the ID to the
        // response.
        .layer(PropagateRequestIdLayer::new(REQUEST_ID))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<_>| {
                    let id = request.headers().get(&REQUEST_ID).and_then(|v| v.to_str().ok()).unwrap_or("-");
                    // Only method and path are logged: no query, headers or bodies,
                    // so tokens and passwords never reach the log.
                    tracing::info_span!("request", request_id = %id, method = %request.method(), path = %request.uri().path())
                })
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(SetRequestIdLayer::new(REQUEST_ID, MakeRequestUuid))
        .with_state(state)
}

/// Opens the database (creating its folder if needed) and runs migrations.
pub async fn open_app(config: &Config) -> Result<App, Box<dyn std::error::Error>> {
    let folder = database_path(&config.database_url).and_then(Path::parent);
    if let Some(folder) = folder.filter(|f| !f.as_os_str().is_empty()) {
        std::fs::create_dir_all(folder)?;
    }
    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;
    Ok(App::new(pool, Arc::new(SystemClock)).with_data_dir(config.data_dir.clone()))
}

/// `sqlite:///data/saldo.db?mode=rwc` → `/data/saldo.db`; `None` for in-memory.
fn database_path(url: &str) -> Option<&Path> {
    let rest = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))?;
    let path = rest
        .split('?')
        .next()
        .filter(|p| !p.is_empty() && !p.contains(":memory:"))?;
    Some(Path::new(path))
}

/// Runs the server until Ctrl-C or SIGTERM, then finishes open requests.
///
/// `tokio::net::TcpListener` and `axum::serve` run on the Tokio runtime that
/// `#[tokio::main]` started: Go's scheduler, but a library you start yourself.
pub async fn serve(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let app = open_app(&config).await?;
    let pool = app.pool.clone();
    let router = build_router(AppState::new(app, config.clone()));
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    info!(bind = %config.bind, public_url = %config.public_url, "saldo listening");
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    // Close the pool so SQLite checkpoints the WAL file.
    tokio::time::timeout(Duration::from_secs(5), pool.close())
        .await
        .ok();
    info!("saldo stopped");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "cannot listen for Ctrl-C");
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => tracing::error!(%error, "cannot listen for SIGTERM"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    // `select!` waits for whichever finishes first, like Go's `select`.
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    info!("shutting down");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_database_file_in_the_url() {
        assert_eq!(
            database_path("sqlite:///data/saldo.db"),
            Some(Path::new("/data/saldo.db"))
        );
        assert_eq!(
            database_path("sqlite://saldo.db?mode=rwc"),
            Some(Path::new("saldo.db"))
        );
        assert_eq!(database_path("sqlite::memory:"), None);
        assert_eq!(database_path("postgres://x"), None);
    }
}
