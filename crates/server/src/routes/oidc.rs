//! `/auth/methods` and the "Log in with Authentik" redirects. The browser
//! navigates to `start`, the provider sends it back to `callback`, and the
//! callback ends with the normal session cookie and a redirect into the app.
//! Failures land on `/login?error=<code>` (details only in the log).

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, header};
use axum::response::{AppendHeaders, IntoResponse, Redirect, Response};
use saldo_app::error::AppError;
use saldo_app::identity::{LoginResult, SESSION_LIFETIME};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::{cookie_value, session_cookie};
use crate::envelope::Data;
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::ApiJson;
use crate::oidc::{self, DesktopReturn, OidcError};
use crate::routes::auth::LoginResponse;
use crate::state::AppState;

/// Ties a login to the browser that started it (defeats login CSRF).
const STATE_COOKIE: &str = "saldo_oidc";
const STATE_COOKIE_PATH: &str = "/api/v1/auth/oidc";
const STATE_MAX_AGE_SECS: i64 = 600;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(methods))
        .routes(routes!(start))
        .routes(routes!(callback))
        .routes(routes!(desktop_token))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthMethods {
    /// Email and password login is allowed.
    pub password: bool,
    /// The identity provider, if "Log in with ..." is configured.
    pub oidc: Option<OidcMethod>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OidcMethod {
    /// For the button: "Log in with <name>".
    pub name: String,
}

/// How people can log in here (public: the login page asks before login).
#[utoipa::path(get, path = "/auth/methods", tag = "auth",
    responses((status = 200, body = Data<AuthMethods>)))]
pub async fn methods(State(state): State<AppState>) -> Json<Data<AuthMethods>> {
    let oidc = state.config.oidc.as_ref().map(|c| OidcMethod {
        name: c.name.clone(),
    });
    Json(Data::new(AuthMethods {
        password: state.config.password_login,
        oidc,
    }))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct StartQuery {
    /// Where in the app to go after login (a path such as `/accounts`).
    pub redirect: Option<String>,
    /// Desktop app only: the loopback port it waits on (1024 or higher).
    pub desktop_port: Option<u16>,
    /// Desktop app only: base64url SHA-256 of its secret verifier.
    pub desktop_challenge: Option<String>,
}

/// Starts "Log in with Authentik": redirects the browser to the provider.
#[utoipa::path(get, path = "/auth/oidc/start", tag = "auth", params(StartQuery),
    responses((status = 303, description = "To the identity provider, or back to /login with an error")))]
pub async fn start(State(state): State<AppState>, Query(query): Query<StartQuery>) -> Response {
    let Some(config) = state.config.oidc.as_ref() else {
        return to_login("oidc_disabled");
    };
    let redirect = safe_path(query.redirect.as_deref());
    let Ok(desktop) = DesktopReturn::parse(query.desktop_port, query.desktop_challenge.as_deref())
    else {
        return to_login("oidc_bad_request");
    };
    match oidc::start(config, &callback_url(&state), redirect).await {
        Ok((url, login_state, pending)) => {
            let pending = pending.for_desktop(desktop);
            state.oidc_logins.insert(login_state.clone(), pending);
            let cookie = state_cookie(&login_state, STATE_MAX_AGE_SECS, state.config.cookie_secure);
            (
                AppendHeaders([(header::SET_COOKIE, cookie)]),
                Redirect::to(&url),
            )
                .into_response()
        }
        Err(error) => failed(&error),
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    /// Set by the provider when the person cancelled or was refused.
    pub error: Option<String>,
}

/// Where the provider sends the browser back. Logs the person in.
#[utoipa::path(get, path = "/auth/oidc/callback", tag = "auth", params(CallbackQuery),
    responses((status = 303, description = "Into the app with a session cookie, or back to /login with an error")))]
pub async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let Some(config) = state.config.oidc.as_ref() else {
        return to_login("oidc_disabled");
    };
    let login_state = query.state.unwrap_or_default();
    let same_browser = cookie_value(&headers, STATE_COOKIE) == Some(login_state.as_str());
    let pending = state
        .oidc_logins
        .take(&login_state)
        .filter(|_| same_browser);
    let desktop = pending.as_ref().and_then(|p| p.desktop.clone());
    // Cancelled or refused at the provider: tell whoever is waiting.
    let refused = query.error.map(OidcError::Refused);
    let (Some(pending), Some(code), None) = (pending, query.code, &refused) else {
        let error = refused.unwrap_or(OidcError::UnknownState);
        return match desktop {
            Some(app) => Redirect::to(&app.url("error", error.code())).into_response(),
            None => failed(&error),
        };
    };
    let redirect = pending.redirect.clone();
    match finish_login(&state, config, pending, code).await {
        Ok(login) => match desktop {
            // The desktop app collects the session with its verifier.
            Some(app) => {
                let code = state.desktop_handoffs.insert(login, app.challenge.clone());
                Redirect::to(&app.url("code", &code)).into_response()
            }
            None => with_session(&state, &login.token, &redirect),
        },
        Err(code) => match desktop {
            Some(app) => Redirect::to(&app.url("error", code)).into_response(),
            None => to_login(code),
        },
    }
}

/// Checks the provider's answer and opens a session for the member; on
/// failure, the error code for the login page.
async fn finish_login(
    state: &AppState,
    config: &oidc::OidcConfig,
    pending: oidc::Pending,
    code: String,
) -> Result<LoginResult, &'static str> {
    let identity = oidc::finish(config, &callback_url(state), pending, code)
        .await
        .map_err(|error| {
            tracing::warn!(%error, "external login failed");
            error.code()
        })?;
    state
        .app
        .identity
        .login_external(identity)
        .await
        .map_err(|error| match error {
            AppError::Unauthorized => "oidc_no_account",
            other => {
                tracing::error!(error = %other, "external login failed");
                "oidc_unavailable"
            }
        })
}

/// The browser's session cookie, the state cookie cleared, and on into the app.
fn with_session(state: &AppState, token: &str, redirect: &str) -> Response {
    let secure = state.config.cookie_secure;
    let session = session_cookie(token, SESSION_LIFETIME.num_seconds(), secure);
    let cleared = state_cookie("", 0, secure);
    let cookies = AppendHeaders([(header::SET_COOKIE, session), (header::SET_COOKIE, cleared)]);
    (cookies, Redirect::to(redirect)).into_response()
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DesktopTokenRequest {
    /// The one-time code the app received on its loopback port.
    pub code: String,
    /// The secret whose SHA-256 the app sent as `desktop_challenge`.
    pub verifier: String,
}

/// The desktop app trades its one-time code and verifier for the session
/// token (a code works once, for two minutes).
#[utoipa::path(post, path = "/auth/oidc/desktop-token", tag = "auth", request_body = DesktopTokenRequest,
    responses((status = 200, body = Data<LoginResponse>), (status = 401, body = ErrorEnvelope)))]
pub async fn desktop_token(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<DesktopTokenRequest>,
) -> Result<Json<Data<LoginResponse>>, ApiError> {
    let login = state
        .desktop_handoffs
        .redeem(&body.code, &body.verifier)
        .ok_or(AppError::Unauthorized)?;
    Ok(Json(Data::new(LoginResponse {
        household_id: login.ctx.household_id.to_string(),
        role: login.ctx.role,
        user: login.user,
        expires_at: login.expires_at,
        token: Some(login.token),
    })))
}

fn callback_url(state: &AppState) -> String {
    format!("{}/api/v1/auth/oidc/callback", state.config.public_url)
}

fn state_cookie(value: &str, max_age: i64, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!(
        "{STATE_COOKIE}={value}; Path={STATE_COOKIE_PATH}; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"
    )
}

fn failed(error: &OidcError) -> Response {
    tracing::warn!(%error, "external login failed");
    to_login(error.code())
}

fn to_login(code: &str) -> Response {
    Redirect::to(&format!("/login?error={code}")).into_response()
}

/// A path inside this site, never another host (`//evil.example`, `https://`).
fn safe_path(redirect: Option<&str>) -> String {
    match redirect {
        Some(path) if path.starts_with('/') && !path.starts_with("//") && !path.contains('\\') => {
            path.to_string()
        }
        _ => "/".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirects_stay_on_this_site() {
        assert_eq!(safe_path(Some("/accounts?x=1")), "/accounts?x=1");
        assert_eq!(safe_path(Some("//evil.example")), "/");
        assert_eq!(safe_path(Some("https://evil.example")), "/");
        assert_eq!(safe_path(Some("/\\evil.example")), "/");
        assert_eq!(safe_path(None), "/");
    }
}
