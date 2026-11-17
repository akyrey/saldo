//! `/tags`: CRUD, plus finding and merging tags whose names differ only in case.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use saldo_app::ctx::Origin;
use saldo_app::tags::{NewTag, TagDuplicateGroup, TagMerge, TagUpdate, TagView};
use saldo_core::TagId;
use serde::Deserialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::categories::PageQuery;
use super::{double_option, page};
use crate::auth::Auth;
use crate::envelope::{Data, List};
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_tags, create_tag))
        .routes(routes!(get_tag, update_tag, delete_tag))
        .routes(routes!(list_duplicates))
        .routes(routes!(merge_tags))
}

#[utoipa::path(get, path = "/tags", tag = "tags", params(PageQuery),
    responses((status = 200, body = List<TagView>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn list_tags(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(query): ApiQuery<PageQuery>,
) -> Result<Json<List<TagView>>, ApiError> {
    let result = state
        .app
        .tags
        .list(&auth.ctx, page(query.page, query.per_page)?)
        .await?;
    Ok(Json(result.into()))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTagRequest {
    pub name: String,
    pub description: Option<String>,
}

#[utoipa::path(post, path = "/tags", tag = "tags", request_body = CreateTagRequest,
    responses((status = 201, body = Data<TagView>), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn create_tag(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<CreateTagRequest>,
) -> Result<(StatusCode, Json<Data<TagView>>), ApiError> {
    let input = NewTag {
        name: body.name,
        description: body.description,
    };
    let view = state.app.tags.create(&auth.ctx, Origin::Api, input).await?;
    Ok((StatusCode::CREATED, Json(Data::new(view))))
}

#[utoipa::path(get, path = "/tags/{id}", tag = "tags", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<TagView>), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn get_tag(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<TagView>>, ApiError> {
    Ok(Json(Data::new(
        state.app.tags.get(&auth.ctx, TagId(id)).await?,
    )))
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct TagPatch {
    pub name: Option<String>,
    /// `null` clears the description.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub description: Option<Option<String>>,
}

#[utoipa::path(patch, path = "/tags/{id}", tag = "tags", params(("id" = Uuid, Path)), request_body = TagPatch,
    responses((status = 200, body = Data<TagView>), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn update_tag(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<TagPatch>,
) -> Result<Json<Data<TagView>>, ApiError> {
    let update = TagUpdate {
        name: body.name,
        description: body.description,
    };
    Ok(Json(Data::new(
        state
            .app
            .tags
            .update(&auth.ctx, Origin::Api, TagId(id), update)
            .await?,
    )))
}

/// Deletes the tag and removes it from every split.
#[utoipa::path(delete, path = "/tags/{id}", tag = "tags", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_tag(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .tags
        .delete(&auth.ctx, Origin::Api, TagId(id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Groups of tags whose names differ only in case, ordered by name; within a
/// group the most used tag comes first.
#[utoipa::path(get, path = "/tags/duplicates", tag = "tags",
    responses((status = 200, body = Data<Vec<TagDuplicateGroup>>), (status = 401, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn list_duplicates(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<Data<Vec<TagDuplicateGroup>>>, ApiError> {
    let groups = state.app.tags.case_duplicates(&auth.ctx).await?;
    Ok(Json(Data::new(groups)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MergeTagsRequest {
    pub merges: Vec<TagMergeRequest>,
}

/// Fold every tag in `sources` into `into`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct TagMergeRequest {
    pub into: Uuid,
    pub sources: Vec<Uuid>,
}

/// Moves every use of the source tags (splits, recurrences, templates, rules,
/// attachments) to the target, then deletes the sources; all merges in one
/// transaction. Returns the target tags in request order.
#[utoipa::path(post, path = "/tags/merge", tag = "tags", request_body = MergeTagsRequest,
    responses((status = 200, body = Data<Vec<TagView>>), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn merge_tags(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<MergeTagsRequest>,
) -> Result<Json<Data<Vec<TagView>>>, ApiError> {
    let merges = body
        .merges
        .into_iter()
        .map(|m| TagMerge {
            into: TagId(m.into),
            sources: m.sources.into_iter().map(TagId).collect(),
        })
        .collect();
    let targets = state.app.tags.merge(&auth.ctx, Origin::Api, merges).await?;
    Ok(Json(Data::new(targets)))
}
