//! `/auth/methods` and the "Log in with Authentik" redirects. The browser
//! navigates to `start`, the provider sends it back to `callback`, and the
//! callback ends with the normal session cookie and a redirect into the app.
//! Failures land on `/login?error=<code>` (details only in the log).

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, header};
use axum::response::{AppendHeaders, IntoResponse, Redirect, Response};
use saldo_app::error::AppError;
use saldo_app::identity::SESSION_LIFETIME;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::{cookie_value, session_cookie};
use crate::envelope::Data;
use crate::oidc::{self, OidcError};
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
}

/// Starts "Log in with Authentik": redirects the browser to the provider.
#[utoipa::path(get, path = "/auth/oidc/start", tag = "auth", params(StartQuery),
    responses((status = 303, description = "To the identity provider, or back to /login with an error")))]
pub async fn start(State(state): State<AppState>, Query(query): Query<StartQuery>) -> Response {
    let Some(config) = state.config.oidc.as_ref() else {
        return to_login("oidc_disabled");
    };
    let redirect = safe_path(query.redirect.as_deref());
    match oidc::start(config, &callback_url(&state), redirect).await {
        Ok((url, login_state, pending)) => {
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
    if let Some(error) = query.error.as_deref() {
        return failed(&OidcError::Refused(error.to_string()));
    }
    let (Some(code), Some(login_state)) = (query.code, query.state) else {
        return failed(&OidcError::UnknownState);
    };
    let same_browser = cookie_value(&headers, STATE_COOKIE) == Some(login_state.as_str());
    let Some(pending) = state
        .oidc_logins
        .take(&login_state)
        .filter(|_| same_browser)
    else {
        return failed(&OidcError::UnknownState);
    };
    let redirect = pending.redirect.clone();
    let identity = match oidc::finish(config, &callback_url(&state), pending, code).await {
        Ok(identity) => identity,
        Err(error) => return failed(&error),
    };
    match state.app.identity.login_external(identity).await {
        Ok(login) => {
            let secure = state.config.cookie_secure;
            let session = session_cookie(&login.token, SESSION_LIFETIME.num_seconds(), secure);
            let cleared = state_cookie("", 0, secure);
            let cookies =
                AppendHeaders([(header::SET_COOKIE, session), (header::SET_COOKIE, cleared)]);
            (cookies, Redirect::to(&redirect)).into_response()
        }
        Err(AppError::Unauthorized) => to_login("oidc_no_account"),
        Err(error) => {
            tracing::error!(%error, "external login failed");
            to_login("oidc_unavailable")
        }
    }
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
