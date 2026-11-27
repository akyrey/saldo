//! `/export`: the household as JSON or as a zip of CSV files, and a snapshot
//! of the whole instance when the server allows it (owner only).

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::response::Response;
use saldo_app::export::ExportDocument;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::Auth;
use crate::error::{ApiError, ErrorEnvelope};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(export_json))
        .routes(routes!(export_csv))
        .routes(routes!(export_snapshot))
}

/// A download response with a dated file name.
fn download(body: Body, content_type: &'static str, name: &str) -> Response {
    let mut response = Response::new(body);
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    response
}

fn file_name(exported_at: &str, extension: &str) -> String {
    format!(
        "saldo-export-{}.{extension}",
        exported_at.get(..10).unwrap_or("now")
    )
}

/// Everything of the household in one JSON document (format `saldo-export`, version 1).
#[utoipa::path(get, path = "/export/json", tag = "export",
    responses((status = 200, body = ExportDocument), (status = 403, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn export_json(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Response, ApiError> {
    let document = state.app.export.json(&auth.ctx).await?;
    let bytes = serde_json::to_vec_pretty(&document)
        .map_err(|e| ApiError::App(saldo_app::error::AppError::Internal(e.to_string())))?;
    Ok(download(
        Body::from(bytes),
        "application/json",
        &file_name(&document.exported_at, "json"),
    ))
}

/// A zip with one CSV file per entity; `transactions.csv` has one row per split.
#[utoipa::path(get, path = "/export/csv", tag = "export",
    responses((status = 200, description = "Zip archive", content_type = "application/zip"), (status = 403, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn export_csv(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Response, ApiError> {
    let bytes = state.app.export.csv_zip(&auth.ctx).await?;
    let today = saldo_app::clock::format_timestamp(state.app.clock.now());
    Ok(download(
        Body::from(bytes),
        "application/zip",
        &file_name(&today, "zip"),
    ))
}

/// The whole instance (database and attachment files) as a zip, to restore
/// into a fresh Saldo with `saldo restore`. Off (404) unless
/// `SALDO_SNAPSHOT_DOWNLOAD=true`; 409 while the instance has another household.
#[utoipa::path(get, path = "/export/snapshot", tag = "export",
    responses((status = 200, description = "Zip archive: saldo.db and attachments/", content_type = "application/zip"),
        (status = 403, body = ErrorEnvelope), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn export_snapshot(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Response, ApiError> {
    if !state.config.snapshot_download {
        return Err(ApiError::App(saldo_app::error::AppError::NotFound));
    }
    let file = state.app.snapshots.create(&auth.ctx).await?;
    let length = file.metadata().map(|m| m.len()).ok();
    // The file is streamed in chunks rather than read into memory: a
    // snapshot holds every attachment and can be large.
    let stream = tokio_util::io::ReaderStream::new(tokio::fs::File::from_std(file));
    let today = saldo_app::clock::format_timestamp(state.app.clock.now());
    let name = format!("saldo-snapshot-{}.zip", today.get(..10).unwrap_or("now"));
    let mut response = download(Body::from_stream(stream), "application/zip", &name);
    if let Some(length) = length {
        response
            .headers_mut()
            .insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    }
    Ok(response)
}
