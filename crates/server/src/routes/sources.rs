//! `/source-documents` and `/source-rows`: Amazon and PayPal exports whose
//! rows explain bank lines (Phase 13).

use axum::Json;
use axum::extract::{DefaultBodyLimit, Multipart, State};
use axum::http::StatusCode;
use chrono::NaiveDate;
use saldo_app::ctx::Origin;
use saldo_app::error::AppError;
use saldo_app::sources::{DocumentView, RowFilter, SourceRowView, SourceUpload};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::page;
use super::statement_profiles::{BODY_LIMIT, read_form};
use crate::auth::Auth;
use crate::envelope::{Data, List};
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiPath, ApiQuery};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_documents, upload_document))
        .routes(routes!(delete_document))
        .routes(routes!(list_rows))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
}

/// The household's uploads, newest first (unpaginated: a few a year).
#[utoipa::path(get, path = "/source-documents", tag = "sources",
    responses((status = 200, body = Data<Vec<DocumentView>>)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn list_documents(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<Data<Vec<DocumentView>>>, ApiError> {
    Ok(Json(Data::new(
        state.app.sources.documents(&auth.ctx).await?,
    )))
}

/// The upload form (documentation only; the handler reads the parts).
#[derive(ToSchema)]
#[allow(dead_code)]
pub struct SourceForm {
    /// `amazon_orders` or `paypal`.
    kind: String,
    /// At most 5 MB.
    #[schema(value_type = String, format = Binary)]
    file: Vec<u8>,
}

/// Stores an Amazon or PayPal export: new rows are added, known ones
/// refreshed. The same file twice is a 409.
#[utoipa::path(post, path = "/source-documents", tag = "sources",
    request_body(content = SourceForm, content_type = "multipart/form-data"),
    responses((status = 201, body = Data<DocumentView>), (status = 400, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn upload_document(
    State(state): State<AppState>,
    Auth(auth): Auth,
    multipart: Multipart,
) -> Result<(StatusCode, Json<Data<DocumentView>>), ApiError> {
    let form = read_form(multipart).await?;
    let (filename, bytes) = form.file()?;
    let kind = form
        .text("kind")
        .ok_or_else(|| AppError::invalid("kind", "must be amazon_orders or paypal"))?;
    let input = SourceUpload {
        kind,
        filename,
        bytes,
    };
    let view = state
        .app
        .sources
        .upload(&auth.ctx, Origin::Api, input)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(view))))
}

/// Deletes an upload and the rows no other upload holds (409 when such a
/// row explains a booked transaction).
#[utoipa::path(delete, path = "/source-documents/{id}", tag = "sources", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_document(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .sources
        .delete_document(&auth.ctx, Origin::Api, &id.to_string())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RowQuery {
    /// `amazon_orders` or `paypal`.
    pub kind: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    /// Only rows that explain nothing yet.
    pub unmatched: Option<bool>,
    pub page: Option<u32>,
    /// At most 200; default 50.
    pub per_page: Option<u32>,
}

/// Stored rows by date.
#[utoipa::path(get, path = "/source-rows", tag = "sources", params(RowQuery),
    responses((status = 200, body = List<SourceRowView>), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn list_rows(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(query): ApiQuery<RowQuery>,
) -> Result<Json<List<SourceRowView>>, ApiError> {
    let filter = RowFilter {
        kind: query.kind,
        from: query.from,
        to: query.to,
        unmatched_only: query.unmatched.unwrap_or(false),
    };
    let result = state
        .app
        .sources
        .rows(&auth.ctx, filter, page(query.page, query.per_page)?)
        .await?;
    Ok(Json(result.into()))
}
