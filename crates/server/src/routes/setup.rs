//! `POST /setup`: the first user and household. Works only once.
//! `GET /setup`: whether that has happened yet.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use saldo_app::ctx::Origin;
use saldo_app::identity::{HouseholdView, SetupInput, UserView};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::envelope::Data;
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::ApiJson;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(setup_status, setup))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetupRequest {
    pub email: String,
    pub password: String,
    pub display_name: String,
    pub household_name: String,
    /// ISO code such as `EUR`.
    pub primary_currency: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SetupResponse {
    pub user: UserView,
    pub household: HouseholdView,
}

/// Whether the first-run setup is still needed.
#[derive(Debug, Serialize, ToSchema)]
pub struct SetupStatus {
    /// True while no user exists.
    pub needed: bool,
}

/// Tells the web app whether to show the setup screen (no authentication).
#[utoipa::path(get, path = "/setup", tag = "auth", responses((status = 200, body = Data<SetupStatus>)))]
pub async fn setup_status(
    State(state): State<AppState>,
) -> Result<Json<Data<SetupStatus>>, ApiError> {
    let needed = state.app.identity.needs_setup().await?;
    Ok(Json(Data::new(SetupStatus { needed })))
}

/// Creates the first user and the household (no authentication; only while
/// no user exists).
#[utoipa::path(post, path = "/setup", tag = "auth", request_body = SetupRequest,
    responses((status = 201, body = Data<SetupResponse>), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)))]
pub async fn setup(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<SetupRequest>,
) -> Result<(StatusCode, Json<Data<SetupResponse>>), ApiError> {
    let input = SetupInput {
        email: body.email,
        password: body.password,
        display_name: body.display_name,
        household_name: body.household_name,
        primary_currency: body.primary_currency,
    };
    let result = state.app.identity.setup(Origin::Api, input).await?;
    let response = SetupResponse {
        user: result.user,
        household: result.household,
    };
    Ok((StatusCode::CREATED, Json(Data::new(response))))
}
