//! `/net-worth-goals`: targets for the household's (or a member's) net worth.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::NaiveDate;
use saldo_app::ctx::Origin;
use saldo_app::goals::{GoalUpdate, NetWorthGoalView, NewGoal};
use saldo_core::UserId;
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
        .routes(routes!(list_goals, create_goal))
        .routes(routes!(get_goal, update_goal, delete_goal))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GoalQuery {
    pub active: Option<bool>,
}

/// Every goal, by name, with its progress today (unpaginated).
#[utoipa::path(get, path = "/net-worth-goals", tag = "goals", params(GoalQuery),
    responses((status = 200, body = Data<Vec<NetWorthGoalView>>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn list_goals(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(query): ApiQuery<GoalQuery>,
) -> Result<Json<Data<Vec<NetWorthGoalView>>>, ApiError> {
    Ok(Json(Data::new(
        state.app.goals.list(&auth.ctx, query.active).await?,
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateGoalRequest {
    pub name: String,
    /// A decimal string; below today's net worth for paying debt off.
    pub target_amount: String,
    /// Default: the household's primary currency.
    pub currency: Option<String>,
    /// `YYYY-MM-DD`.
    pub target_date: Option<NaiveDate>,
    /// Count one member's accounts only; default the whole household.
    pub owner_user_id: Option<Uuid>,
    pub notes: Option<String>,
}

/// Creates a goal starting from today's net worth.
#[utoipa::path(post, path = "/net-worth-goals", tag = "goals", request_body = CreateGoalRequest,
    responses((status = 201, body = Data<NetWorthGoalView>), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn create_goal(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<CreateGoalRequest>,
) -> Result<(StatusCode, Json<Data<NetWorthGoalView>>), ApiError> {
    let input = NewGoal {
        name: body.name,
        owner_user_id: body.owner_user_id.map(UserId),
        target_amount: body.target_amount,
        currency: body.currency,
        target_date: body.target_date,
        notes: body.notes,
    };
    let view = state
        .app
        .goals
        .create(&auth.ctx, Origin::Api, input)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(view))))
}

#[utoipa::path(get, path = "/net-worth-goals/{id}", tag = "goals", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<NetWorthGoalView>), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn get_goal(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<NetWorthGoalView>>, ApiError> {
    Ok(Json(Data::new(state.app.goals.get(&auth.ctx, id).await?)))
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct GoalPatch {
    pub name: Option<String>,
    /// A new target makes the goal able to alert again.
    pub target_amount: Option<String>,
    /// `null` removes the target date.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<NaiveDate>)]
    pub target_date: Option<Option<NaiveDate>>,
    pub active: Option<bool>,
    /// `null` clears the notes.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub notes: Option<Option<String>>,
}

#[utoipa::path(patch, path = "/net-worth-goals/{id}", tag = "goals", params(("id" = Uuid, Path)), request_body = GoalPatch,
    responses((status = 200, body = Data<NetWorthGoalView>), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn update_goal(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<GoalPatch>,
) -> Result<Json<Data<NetWorthGoalView>>, ApiError> {
    let update = GoalUpdate {
        name: body.name,
        target_amount: body.target_amount,
        target_date: body.target_date,
        active: body.active,
        notes: body.notes,
    };
    let view = state
        .app
        .goals
        .update(&auth.ctx, Origin::Api, id, update)
        .await?;
    Ok(Json(Data::new(view)))
}

#[utoipa::path(delete, path = "/net-worth-goals/{id}", tag = "goals", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_goal(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.app.goals.delete(&auth.ctx, Origin::Api, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
