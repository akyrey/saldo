//! `/rule-groups` and `/rules`: rules that change transactions when saved.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use saldo_app::ctx::Origin;
use saldo_app::rules::{
    ActionView, NewRuleGroup, RuleApplyResult, RuleGroupUpdate, RuleGroupView, RuleInput,
    RuleTestResult, RuleView, TriggerView,
};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::double_option;
use crate::auth::Auth;
use crate::envelope::Data;
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_groups, create_group))
        .routes(routes!(get_group, update_group, delete_group))
        .routes(routes!(list_rules, create_rule))
        .routes(routes!(get_rule, replace_rule, delete_rule))
        .routes(routes!(test_rule))
        .routes(routes!(apply_rule))
}

/// Rule groups in evaluation order.
#[utoipa::path(get, path = "/rule-groups", tag = "rules",
    responses((status = 200, body = Data<Vec<RuleGroupView>>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn list_groups(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<Data<Vec<RuleGroupView>>>, ApiError> {
    Ok(Json(Data::new(
        state.app.rules.list_groups(&auth.ctx).await?,
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateRuleGroupRequest {
    pub title: String,
    pub description: Option<String>,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default = "yes")]
    pub active: bool,
}

fn yes() -> bool {
    true
}

#[utoipa::path(post, path = "/rule-groups", tag = "rules", request_body = CreateRuleGroupRequest,
    responses((status = 201, body = Data<RuleGroupView>), (status = 422, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn create_group(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<CreateRuleGroupRequest>,
) -> Result<(StatusCode, Json<Data<RuleGroupView>>), ApiError> {
    let input = NewRuleGroup {
        title: body.title,
        description: body.description,
        sort_order: body.sort_order,
        active: body.active,
    };
    Ok((
        StatusCode::CREATED,
        Json(Data::new(
            state
                .app
                .rules
                .create_group(&auth.ctx, Origin::Api, input)
                .await?,
        )),
    ))
}

#[utoipa::path(get, path = "/rule-groups/{id}", tag = "rules", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<RuleGroupView>), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn get_group(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<RuleGroupView>>, ApiError> {
    Ok(Json(Data::new(
        state
            .app
            .rules
            .get_group(&auth.ctx, &id.to_string())
            .await?,
    )))
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct RuleGroupPatch {
    pub title: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub description: Option<Option<String>>,
    pub sort_order: Option<i64>,
    pub active: Option<bool>,
}

#[utoipa::path(patch, path = "/rule-groups/{id}", tag = "rules", params(("id" = Uuid, Path)), request_body = RuleGroupPatch,
    responses((status = 200, body = Data<RuleGroupView>), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn update_group(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<RuleGroupPatch>,
) -> Result<Json<Data<RuleGroupView>>, ApiError> {
    let update = RuleGroupUpdate {
        title: body.title,
        description: body.description,
        sort_order: body.sort_order,
        active: body.active,
    };
    Ok(Json(Data::new(
        state
            .app
            .rules
            .update_group(&auth.ctx, Origin::Api, &id.to_string(), update)
            .await?,
    )))
}

/// Deletes a group with all its rules.
#[utoipa::path(delete, path = "/rule-groups/{id}", tag = "rules", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_group(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .rules
        .delete_group(&auth.ctx, Origin::Api, &id.to_string())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RuleQuery {
    /// Only the rules of this group.
    pub rule_group_id: Option<Uuid>,
}

/// Rules in evaluation order (group order, then rule order).
#[utoipa::path(get, path = "/rules", tag = "rules", params(RuleQuery),
    responses((status = 200, body = Data<Vec<RuleView>>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn list_rules(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(query): ApiQuery<RuleQuery>,
) -> Result<Json<Data<Vec<RuleView>>>, ApiError> {
    let group = query.rule_group_id.map(|id| id.to_string());
    Ok(Json(Data::new(
        state
            .app
            .rules
            .list_rules(&auth.ctx, group.as_deref())
            .await?,
    )))
}

/// A whole rule (POST creates, PUT replaces).
#[derive(Debug, Deserialize, ToSchema)]
pub struct RuleRequest {
    pub rule_group_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    /// `create`, `update` or `manual`.
    pub trigger_on: String,
    /// All triggers must match (`true`) or any one.
    #[serde(default = "yes")]
    pub strict: bool,
    #[serde(default)]
    pub stop_processing: bool,
    #[serde(default = "yes")]
    pub active: bool,
    #[serde(default)]
    pub sort_order: i64,
    pub triggers: Vec<TriggerView>,
    pub actions: Vec<ActionView>,
}

impl From<RuleRequest> for RuleInput {
    fn from(body: RuleRequest) -> Self {
        RuleInput {
            rule_group_id: body.rule_group_id.to_string(),
            title: body.title,
            description: body.description,
            trigger_on: body.trigger_on,
            strict: body.strict,
            stop_processing: body.stop_processing,
            active: body.active,
            sort_order: body.sort_order,
            triggers: body.triggers,
            actions: body.actions,
        }
    }
}

#[utoipa::path(post, path = "/rules", tag = "rules", request_body = RuleRequest,
    responses((status = 201, body = Data<RuleView>), (status = 422, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn create_rule(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<RuleRequest>,
) -> Result<(StatusCode, Json<Data<RuleView>>), ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(Data::new(
            state
                .app
                .rules
                .create_rule(&auth.ctx, Origin::Api, body.into())
                .await?,
        )),
    ))
}

#[utoipa::path(get, path = "/rules/{id}", tag = "rules", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<RuleView>), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn get_rule(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<RuleView>>, ApiError> {
    Ok(Json(Data::new(
        state.app.rules.get_rule(&auth.ctx, &id.to_string()).await?,
    )))
}

#[utoipa::path(put, path = "/rules/{id}", tag = "rules", params(("id" = Uuid, Path)), request_body = RuleRequest,
    responses((status = 200, body = Data<RuleView>), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn replace_rule(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<RuleRequest>,
) -> Result<Json<Data<RuleView>>, ApiError> {
    Ok(Json(Data::new(
        state
            .app
            .rules
            .replace_rule(&auth.ctx, Origin::Api, &id.to_string(), body.into())
            .await?,
    )))
}

#[utoipa::path(delete, path = "/rules/{id}", tag = "rules", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_rule(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .rules
        .delete_rule(&auth.ctx, Origin::Api, &id.to_string())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Dry run: the splits the rule would change (first 100), nothing is saved.
#[utoipa::path(post, path = "/rules/{id}/test", tag = "rules", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<RuleTestResult>), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn test_rule(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<RuleTestResult>>, ApiError> {
    Ok(Json(Data::new(
        state
            .app
            .rules
            .test_rule(&auth.ctx, &id.to_string())
            .await?,
    )))
}

/// Runs the rule over every existing transaction and saves the changes.
#[utoipa::path(post, path = "/rules/{id}/apply", tag = "rules", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<RuleApplyResult>), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn apply_rule(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<RuleApplyResult>>, ApiError> {
    Ok(Json(Data::new(
        state
            .app
            .rules
            .apply_rule(&auth.ctx, &id.to_string())
            .await?,
    )))
}
