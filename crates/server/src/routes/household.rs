//! `/household`: settings and members.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use saldo_app::ctx::Origin;
use saldo_app::identity::{HouseholdUpdate, HouseholdView, MemberView, NewMember};
use saldo_core::UserId;
use serde::Deserialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Auth;
use crate::envelope::Data;
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiJson, ApiPath};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_household, update_household))
        .routes(routes!(list_members, add_member))
        .routes(routes!(remove_member))
}

#[utoipa::path(get, path = "/household", tag = "household",
    responses((status = 200, body = Data<HouseholdView>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn get_household(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<Data<HouseholdView>>, ApiError> {
    Ok(Json(Data::new(
        state.app.identity.get_household(&auth.ctx).await?,
    )))
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct HouseholdPatch {
    pub name: Option<String>,
    pub timezone: Option<String>,
    pub primary_currency: Option<String>,
}

/// Changes household settings (owner only).
#[utoipa::path(patch, path = "/household", tag = "household", request_body = HouseholdPatch,
    responses((status = 200, body = Data<HouseholdView>), (status = 403, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn update_household(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<HouseholdPatch>,
) -> Result<Json<Data<HouseholdView>>, ApiError> {
    let update = HouseholdUpdate {
        name: body.name,
        timezone: body.timezone,
        primary_currency: body.primary_currency,
    };
    Ok(Json(Data::new(
        state
            .app
            .identity
            .update_household(&auth.ctx, Origin::Api, update)
            .await?,
    )))
}

#[utoipa::path(get, path = "/household/members", tag = "household",
    responses((status = 200, body = Data<Vec<MemberView>>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn list_members(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<Data<Vec<MemberView>>>, ApiError> {
    Ok(Json(Data::new(
        state.app.identity.list_members(&auth.ctx).await?,
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct NewMemberRequest {
    pub email: String,
    pub display_name: String,
    /// The member's initial password.
    pub password: String,
}

/// Adds a member with an initial password (owner only).
#[utoipa::path(post, path = "/household/members", tag = "household", request_body = NewMemberRequest,
    responses((status = 201, body = Data<MemberView>), (status = 403, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn add_member(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<NewMemberRequest>,
) -> Result<(StatusCode, Json<Data<MemberView>>), ApiError> {
    let input = NewMember {
        email: body.email,
        display_name: body.display_name,
        password: body.password,
    };
    let member = state
        .app
        .identity
        .add_member(&auth.ctx, Origin::Api, input)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(member))))
}

/// Removes a member and ends their sessions (owner only).
#[utoipa::path(delete, path = "/household/members/{user_id}", tag = "household", params(("user_id" = Uuid, Path)),
    responses((status = 204), (status = 403, body = ErrorEnvelope), (status = 404, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn remove_member(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(user_id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .identity
        .remove_member(&auth.ctx, Origin::Api, UserId(user_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
