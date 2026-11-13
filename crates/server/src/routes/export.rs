//! `/export`: the household as JSON or as a zip of CSV files (owner only).

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
}

/// A download response with a dated file name.
fn download(bytes: Vec<u8>, content_type: &'static str, name: &str) -> Response {
    let mut response = Response::new(Body::from(bytes));
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
        bytes,
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
        bytes,
        "application/zip",
        &file_name(&today, "zip"),
    ))
}
