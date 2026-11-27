//! `/statement-imports`: upload a statement as a draft, review its lines,
//! commit, discard or revert it (Phase 13).

use axum::Json;
use axum::extract::{DefaultBodyLimit, Multipart, State};
use axum::http::StatusCode;
use saldo_app::ctx::Origin;
use saldo_app::error::AppError;
use saldo_app::statements::{ImportDetail, ImportView, LineUpdate, LineView, UploadInput};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::statement_profiles::{BODY_LIMIT, read_form};
use super::{double_option, page};
use crate::auth::Auth;
use crate::envelope::{Data, List};
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_imports, upload_import))
        .routes(routes!(get_import))
        .routes(routes!(update_line))
        .routes(routes!(commit_import))
        .routes(routes!(discard_import))
        .routes(routes!(revert_import))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ImportQuery {
    /// Only this account's imports.
    pub account_id: Option<Uuid>,
    pub page: Option<u32>,
    /// At most 200; default 50.
    pub per_page: Option<u32>,
}

/// Imports newest first.
#[utoipa::path(get, path = "/statement-imports", tag = "statements", params(ImportQuery),
    responses((status = 200, body = List<ImportView>), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn list_imports(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(query): ApiQuery<ImportQuery>,
) -> Result<Json<List<ImportView>>, ApiError> {
    let result = state
        .app
        .statements
        .list(
            &auth.ctx,
            query.account_id.map(|id| id.to_string()),
            page(query.page, query.per_page)?,
        )
        .await?;
    Ok(Json(result.into()))
}

/// The upload form (documentation only; the handler reads the parts).
#[derive(ToSchema)]
#[allow(dead_code)]
pub struct UploadForm {
    profile_id: Uuid,
    /// The asset or liability account; the profile's when absent.
    account_id: Option<Uuid>,
    /// At most 5 MB; the parts' file name is kept.
    #[schema(value_type = String, format = Binary)]
    file: Vec<u8>,
}

/// Reads a statement with a saved profile and stores it as a draft, every
/// line classified (already imported, skipped before, an existing
/// transaction, flagged in a reconciled period, or new).
#[utoipa::path(post, path = "/statement-imports", tag = "statements",
    request_body(content = UploadForm, content_type = "multipart/form-data"),
    responses((status = 201, body = Data<ImportDetail>), (status = 400, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn upload_import(
    State(state): State<AppState>,
    Auth(auth): Auth,
    multipart: Multipart,
) -> Result<(StatusCode, Json<Data<ImportDetail>>), ApiError> {
    let form = read_form(multipart).await?;
    let (filename, bytes) = form.file()?;
    let profile_id = form
        .text("profile_id")
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| AppError::invalid("profile_id", "choose a profile"))?;
    let input = UploadInput {
        account_id: form.text("account_id").filter(|id| !id.trim().is_empty()),
        profile_id,
        filename,
        bytes,
    };
    let detail = state
        .app
        .statements
        .upload(&auth.ctx, Origin::Api, input)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(detail))))
}

/// An import with its lines in file order.
#[utoipa::path(get, path = "/statement-imports/{id}", tag = "statements", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<ImportDetail>), (status = 404, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn get_import(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<ImportDetail>>, ApiError> {
    Ok(Json(Data::new(
        state.app.statements.get(&auth.ctx, &id.to_string()).await?,
    )))
}

/// Fields to change on a draft line. Missing fields stay; `null` clears.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct LinePatch {
    /// `create`, `link` or `skip`.
    pub action: Option<String>,
    /// The existing split a `link` line is.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<Uuid>)]
    pub split_id: Option<Option<Uuid>>,
    /// Null creates an expense or revenue account named after the line.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<Uuid>)]
    pub counterparty_account_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<Uuid>)]
    pub category_id: Option<Option<Uuid>>,
}

fn text_of(value: Option<Option<Uuid>>) -> Option<Option<String>> {
    value.map(|inner| inner.map(|id| id.to_string()))
}

/// A review edit of one draft line (409 once the import is not a draft).
#[utoipa::path(patch, path = "/statement-imports/{id}/lines/{position}", tag = "statements",
    params(("id" = Uuid, Path), ("position" = i64, Path)), request_body = LinePatch,
    responses((status = 200, body = Data<LineView>), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn update_line(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath((id, position)): ApiPath<(Uuid, i64)>,
    ApiJson(body): ApiJson<LinePatch>,
) -> Result<Json<Data<LineView>>, ApiError> {
    let update = LineUpdate {
        action: body.action,
        split_id: text_of(body.split_id),
        counterparty_account_id: text_of(body.counterparty_account_id),
        category_id: text_of(body.category_id),
    };
    let view = state
        .app
        .statements
        .update_line(&auth.ctx, &id.to_string(), position, update)
        .await?;
    Ok(Json(Data::new(view)))
}

/// Books a draft: `create` lines become transactions (rules run), `link`
/// lines record the link, `skip` lines are remembered.
#[utoipa::path(post, path = "/statement-imports/{id}/commit", tag = "statements", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<ImportDetail>), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn commit_import(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<ImportDetail>>, ApiError> {
    let detail = state
        .app
        .statements
        .commit(&auth.ctx, Origin::Api, &id.to_string())
        .await?;
    Ok(Json(Data::new(detail)))
}

/// Throws a draft away.
#[utoipa::path(post, path = "/statement-imports/{id}/discard", tag = "statements", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<ImportView>), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn discard_import(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<ImportView>>, ApiError> {
    let view = state
        .app
        .statements
        .discard(&auth.ctx, Origin::Api, &id.to_string())
        .await?;
    Ok(Json(Data::new(view)))
}

/// Undoes a committed import: deletes the transactions it created and
/// forgets its links (409 when one of them is reconciled).
#[utoipa::path(post, path = "/statement-imports/{id}/revert", tag = "statements", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<ImportView>), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn revert_import(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<ImportView>>, ApiError> {
    let view = state
        .app
        .statements
        .revert(&auth.ctx, Origin::Api, &id.to_string())
        .await?;
    Ok(Json(Data::new(view)))
}
