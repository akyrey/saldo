//! Authentication: the `Auth` extractor, CSRF check and session cookie.

use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method, header};
use saldo_app::error::AppError;
use saldo_app::identity::Authenticated;

use crate::error::ApiError;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "saldo_session";
pub const CSRF_HEADER: &str = "x-saldo-csrf";

/// The authenticated caller. Adding `Auth(auth): Auth` to a handler's
/// arguments makes the route require a session cookie or bearer token; Axum
/// answers 401 before the handler runs if there is none.
pub struct Auth(pub Authenticated);

// Axum calls this for every handler that takes `Auth`. It is an `async fn`
// in a trait impl: the trait (Axum's interface) says how to build the value.
impl FromRequestParts<AppState> for Auth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(token) = bearer_token(&parts.headers) {
            // Scripts and the desktop app: no cookie, so no CSRF risk.
            return Ok(Auth(state.app.identity.authenticate(token).await?));
        }
        let token = cookie_token(&parts.headers).ok_or(AppError::Unauthorized)?;
        let authenticated = state.app.identity.authenticate(token).await?;
        let reads_only = parts.method == Method::GET || parts.method == Method::HEAD;
        if !reads_only && !csrf_ok(&parts.headers, &state.config.public_origin) {
            return Err(ApiError::CsrfFailed);
        }
        Ok(Auth(authenticated))
    }
}

/// The caller's IP address if the server knows it (not behind a proxy).
pub struct ClientIp(pub Option<IpAddr>);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _: &AppState) -> Result<Self, Self::Rejection> {
        let ip = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|info| info.0.ip());
        Ok(ClientIp(ip))
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then_some(token.trim())
        .filter(|t| !t.is_empty())
}

fn cookie_token(headers: &HeaderMap) -> Option<&str> {
    cookie_value(headers, SESSION_COOKIE)
}

/// The value of cookie `name`, if present and not empty.
pub fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(cookie, _)| *cookie == name)
        .map(|(_, value)| value)
        .filter(|value| !value.is_empty())
}

/// A cookie request that changes data must send `X-Saldo-Csrf: 1` and an
/// `Origin` equal to the public URL's origin.
fn csrf_ok(headers: &HeaderMap, public_origin: &str) -> bool {
    let header_ok = headers.get(CSRF_HEADER).is_some_and(|v| v == "1");
    let origin_ok = headers
        .get(header::ORIGIN)
        .is_some_and(|v| v == public_origin);
    header_ok && origin_ok
}

/// `Set-Cookie` value for a new session.
pub fn session_cookie(token: &str, max_age_seconds: i64, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_seconds}{secure}"
    )
}

/// `Set-Cookie` value that removes the session cookie.
pub fn clear_session_cookie(secure: bool) -> String {
    session_cookie("", 0, secure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(pairs: &[(header::HeaderName, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(name.clone(), HeaderValue::from_str(value).unwrap());
        }
        map
    }

    #[test]
    fn reads_bearer_tokens_case_insensitively() {
        assert_eq!(
            bearer_token(&headers(&[(header::AUTHORIZATION, "Bearer abc")])),
            Some("abc")
        );
        assert_eq!(
            bearer_token(&headers(&[(header::AUTHORIZATION, "bearer abc")])),
            Some("abc")
        );
        assert_eq!(
            bearer_token(&headers(&[(header::AUTHORIZATION, "Basic abc")])),
            None
        );
        assert_eq!(
            bearer_token(&headers(&[(header::AUTHORIZATION, "Bearer ")])),
            None
        );
        assert_eq!(bearer_token(&HeaderMap::new()), None);
    }

    #[test]
    fn finds_the_session_cookie_among_others() {
        let map = headers(&[(header::COOKIE, "a=1; saldo_session=tok; b=2")]);
        assert_eq!(cookie_token(&map), Some("tok"));
        assert_eq!(
            cookie_token(&headers(&[(header::COOKIE, "saldo_session=")])),
            None
        );
        assert_eq!(cookie_token(&headers(&[(header::COOKIE, "other=1")])), None);
    }

    #[test]
    fn csrf_needs_header_and_origin() {
        let origin = "https://money.example.com";
        let both = headers(&[
            (header::ORIGIN, origin),
            (header::HeaderName::from_static(CSRF_HEADER), "1"),
        ]);
        assert!(csrf_ok(&both, origin));
        assert!(!csrf_ok(&headers(&[(header::ORIGIN, origin)]), origin));
        let wrong_origin = headers(&[
            (header::ORIGIN, "https://evil.example"),
            (header::HeaderName::from_static(CSRF_HEADER), "1"),
        ]);
        assert!(!csrf_ok(&wrong_origin, origin));
    }

    #[test]
    fn cookies_are_http_only_and_lax() {
        let cookie = session_cookie("tok", 60, true);
        assert_eq!(
            cookie,
            "saldo_session=tok; Path=/; HttpOnly; SameSite=Lax; Max-Age=60; Secure"
        );
        assert!(!clear_session_cookie(false).contains("Secure"));
        assert!(clear_session_cookie(false).contains("Max-Age=0"));
    }
}
