//! `/auth`: login, logout, the current user, and personal access tokens.

use std::time::Instant;

use axum::Json;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{AppendHeaders, IntoResponse};
use saldo_app::ctx::Role;
use saldo_app::identity::{MeView, SESSION_LIFETIME, TokenView, UserView};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::{Auth, ClientIp, clear_session_cookie, session_cookie};
use crate::envelope::Data;
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiJson, ApiPath};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(login))
        .routes(routes!(logout))
        .routes(routes!(me))
        .routes(routes!(list_tokens, create_token))
        .routes(routes!(revoke_token))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    /// `browser` (default): the session arrives as a cookie. `desktop`: the
    /// session token is returned in the body for `Authorization: Bearer`,
    /// and no cookie is set (the desktop app keeps it in the OS keychain).
    pub client: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResponse {
    pub user: UserView,
    pub household_id: String,
    pub role: Role,
    pub expires_at: String,
    /// Only for `client: "desktop"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

/// Whether the login is for the desktop app; any other client is refused.
fn is_desktop(client: Option<&str>) -> Result<bool, ApiError> {
    match client {
        None | Some("browser") => Ok(false),
        Some("desktop") => Ok(true),
        Some(_) => {
            Err(saldo_app::error::AppError::invalid("client", "must be browser or desktop").into())
        }
    }
}

/// Checks the password and sets the `saldo_session` cookie. Rate-limited per
/// IP and per email.
#[utoipa::path(post, path = "/auth/login", tag = "auth", request_body = LoginRequest,
    responses((status = 200, body = Data<LoginResponse>, headers(("set-cookie" = String))),
              (status = 401, body = ErrorEnvelope), (status = 429, body = ErrorEnvelope)))]
pub async fn login(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    ApiJson(body): ApiJson<LoginRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let desktop = is_desktop(body.client.as_deref())?;
    let mut keys = vec![format!("email:{}", body.email.trim().to_lowercase())];
    if let Some(ip) = ip {
        keys.push(format!("ip:{ip}"));
    }
    if !state.login_limiter.allow(&keys, Instant::now()) {
        return Err(ApiError::TooManyRequests);
    }
    let result = state
        .app
        .identity
        .login(&body.email, &body.password)
        .await?;
    let cookie = session_cookie(
        &result.token,
        SESSION_LIFETIME.num_seconds(),
        state.config.cookie_secure,
    );
    let response = LoginResponse {
        user: result.user,
        household_id: result.ctx.household_id.to_string(),
        role: result.ctx.role,
        expires_at: result.expires_at,
        token: desktop.then(|| result.token.clone()),
    };
    // `Option<AppendHeaders>` adds the header only when it is `Some`.
    let cookie_header = (!desktop).then(|| AppendHeaders([(header::SET_COOKIE, cookie)]));
    Ok((cookie_header, Json(Data::new(response))))
}

/// Ends the current session (or revokes the token used) and clears the cookie.
#[utoipa::path(post, path = "/auth/logout", tag = "auth",
    responses((status = 204), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn logout(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<impl IntoResponse, ApiError> {
    state
        .app
        .identity
        .logout(&auth.ctx, &auth.session_id)
        .await?;
    let cookie = clear_session_cookie(state.config.cookie_secure);
    Ok((
        StatusCode::NO_CONTENT,
        AppendHeaders([(header::SET_COOKIE, cookie)]),
    ))
}

/// The current user, household and role.
#[utoipa::path(get, path = "/auth/me", tag = "auth",
    responses((status = 200, body = Data<MeView>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn me(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<Data<MeView>>, ApiError> {
    Ok(Json(Data::new(state.app.identity.me(&auth.ctx).await?)))
}

/// The caller's personal access tokens (without their secrets).
#[utoipa::path(get, path = "/auth/tokens", tag = "auth",
    responses((status = 200, body = Data<Vec<TokenView>>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn list_tokens(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<Data<Vec<TokenView>>>, ApiError> {
    Ok(Json(Data::new(
        state.app.identity.list_tokens(&auth.ctx).await?,
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTokenRequest {
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreatedTokenResponse {
    /// Shown only now; send it as `Authorization: Bearer <token>`.
    pub token: String,
    pub info: TokenView,
}

/// Creates a personal access token for scripts, the importer or the desktop app.
#[utoipa::path(post, path = "/auth/tokens", tag = "auth", request_body = CreateTokenRequest,
    responses((status = 201, body = Data<CreatedTokenResponse>), (status = 401, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn create_token(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<CreateTokenRequest>,
) -> Result<(StatusCode, Json<Data<CreatedTokenResponse>>), ApiError> {
    let created = state
        .app
        .identity
        .create_token(&auth.ctx, &body.name)
        .await?;
    let response = CreatedTokenResponse {
        token: created.token,
        info: created.view,
    };
    Ok((StatusCode::CREATED, Json(Data::new(response))))
}

/// Revokes one of the caller's tokens.
#[utoipa::path(delete, path = "/auth/tokens/{id}", tag = "auth", params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn revoke_token(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<String>,
) -> Result<StatusCode, ApiError> {
    state.app.identity.revoke_token(&auth.ctx, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
