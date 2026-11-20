//! `/saved-views`: named filters for a list page, shared by the household.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use saldo_app::ctx::Origin;
use saldo_app::saved_views::{NewSavedView, SavedViewUpdate, SavedViewView};
use serde::Deserialize;
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Auth;
use crate::envelope::Data;
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_saved_views, create_saved_view))
        .routes(routes!(update_saved_view, delete_saved_view))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SavedViewQuery {
    /// The list page; only `transactions` for now (the default).
    pub page: Option<String>,
}

/// The page's views, by `sort_order`, then name (unpaginated).
#[utoipa::path(get, path = "/saved-views", tag = "saved-views", params(SavedViewQuery),
    responses((status = 200, body = Data<Vec<SavedViewView>>), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn list_saved_views(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(query): ApiQuery<SavedViewQuery>,
) -> Result<Json<Data<Vec<SavedViewView>>>, ApiError> {
    let page = query.page.as_deref().unwrap_or("transactions");
    let views = state.app.saved_views.list(&auth.ctx, page).await?;
    Ok(Json(Data::new(views)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateSavedViewRequest {
    pub name: String,
    /// Default `transactions`.
    pub page: Option<String>,
    /// A flat object of strings, numbers and booleans: at most 30 keys and 2 KB.
    #[schema(value_type = std::collections::HashMap<String, serde_json::Value>)]
    pub filters: Value,
    #[serde(default)]
    pub sort_order: i64,
}

#[utoipa::path(post, path = "/saved-views", tag = "saved-views", request_body = CreateSavedViewRequest,
    responses((status = 201, body = Data<SavedViewView>), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn create_saved_view(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<CreateSavedViewRequest>,
) -> Result<(StatusCode, Json<Data<SavedViewView>>), ApiError> {
    let input = NewSavedView {
        name: body.name,
        page: body.page.unwrap_or_else(|| "transactions".to_string()),
        filters: body.filters,
        sort_order: body.sort_order,
    };
    let view = state
        .app
        .saved_views
        .create(&auth.ctx, Origin::Api, input)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(view))))
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct SavedViewPatch {
    pub name: Option<String>,
    /// Replaces the stored filters.
    #[schema(value_type = Option<std::collections::HashMap<String, serde_json::Value>>)]
    pub filters: Option<Value>,
    pub sort_order: Option<i64>,
}

#[utoipa::path(patch, path = "/saved-views/{id}", tag = "saved-views", params(("id" = Uuid, Path)), request_body = SavedViewPatch,
    responses((status = 200, body = Data<SavedViewView>), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn update_saved_view(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<SavedViewPatch>,
) -> Result<Json<Data<SavedViewView>>, ApiError> {
    let update = SavedViewUpdate {
        name: body.name,
        filters: body.filters,
        sort_order: body.sort_order,
    };
    let view = state
        .app
        .saved_views
        .update(&auth.ctx, Origin::Api, id, update)
        .await?;
    Ok(Json(Data::new(view)))
}

#[utoipa::path(delete, path = "/saved-views/{id}", tag = "saved-views", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_saved_view(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .saved_views
        .delete(&auth.ctx, Origin::Api, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
