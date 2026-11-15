//! `/categories`: CRUD.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use saldo_app::categories::{CategoryUpdate, CategoryView, NewCategory};
use saldo_app::ctx::Origin;
use saldo_core::CategoryId;
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::{double_option, page};
use crate::auth::Auth;
use crate::envelope::{Data, List};
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_categories, create_category))
        .routes(routes!(get_category, update_category, delete_category))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageQuery {
    pub page: Option<u32>,
    /// At most 200; default 50.
    pub per_page: Option<u32>,
}

#[utoipa::path(get, path = "/categories", tag = "categories", params(PageQuery),
    responses((status = 200, body = List<CategoryView>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn list_categories(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(query): ApiQuery<PageQuery>,
) -> Result<Json<List<CategoryView>>, ApiError> {
    let result = state
        .app
        .categories
        .list(&auth.ctx, page(query.page, query.per_page)?)
        .await?;
    Ok(Json(result.into()))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateCategoryRequest {
    pub name: String,
    pub notes: Option<String>,
    /// red, orange, amber, lime, green, teal, sky, blue, violet, pink or gray.
    pub color: Option<String>,
}

#[utoipa::path(post, path = "/categories", tag = "categories", request_body = CreateCategoryRequest,
    responses((status = 201, body = Data<CategoryView>), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn create_category(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<CreateCategoryRequest>,
) -> Result<(StatusCode, Json<Data<CategoryView>>), ApiError> {
    let input = NewCategory {
        name: body.name,
        notes: body.notes,
        color: body.color,
    };
    let view = state
        .app
        .categories
        .create(&auth.ctx, Origin::Api, input)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(view))))
}

#[utoipa::path(get, path = "/categories/{id}", tag = "categories", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<CategoryView>), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn get_category(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<CategoryView>>, ApiError> {
    Ok(Json(Data::new(
        state.app.categories.get(&auth.ctx, CategoryId(id)).await?,
    )))
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct CategoryPatch {
    pub name: Option<String>,
    /// `null` clears the notes.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub notes: Option<Option<String>>,
    /// A palette colour name (see `POST /categories`); `null` clears it.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub color: Option<Option<String>>,
}

#[utoipa::path(patch, path = "/categories/{id}", tag = "categories", params(("id" = Uuid, Path)), request_body = CategoryPatch,
    responses((status = 200, body = Data<CategoryView>), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn update_category(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<CategoryPatch>,
) -> Result<Json<Data<CategoryView>>, ApiError> {
    let update = CategoryUpdate {
        name: body.name,
        notes: body.notes,
        color: body.color,
    };
    Ok(Json(Data::new(
        state
            .app
            .categories
            .update(&auth.ctx, Origin::Api, CategoryId(id), update)
            .await?,
    )))
}

/// Deletes the category; splits that used it keep no category.
#[utoipa::path(delete, path = "/categories/{id}", tag = "categories", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_category(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .categories
        .delete(&auth.ctx, Origin::Api, CategoryId(id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
