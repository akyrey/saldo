//! Serving the web app (the single-page app in `web/`) for every path
//! outside `/api`.
//!
//! The files are compiled into the binary only with the `embed-web` cargo
//! feature, which works like a Go build tag. `rust-embed` is Go's
//! `embed.FS`: `cargo build --features embed-web` reads `web/dist` at
//! compile time. Without the feature the server is API-only.
//!
//! The decision logic below takes a lookup function instead of the embedded
//! folder, so it is tested in every build.

use std::borrow::Cow;

use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use saldo_app::error::AppError;

use crate::error::ApiError;

/// Hashed build output: the name changes whenever the content does.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// `index.html`, the service worker and the manifest must be revalidated,
/// or browsers keep running an old version of the app.
const REVALIDATE: &str = "no-cache";

/// A file found by a lookup: its bytes and MIME type.
pub struct Asset {
    pub bytes: Cow<'static, [u8]>,
    pub mime: String,
}

/// The `Cache-Control` value for a file path relative to the build folder.
pub fn cache_control(path: &str) -> &'static str {
    if path.starts_with("assets/") {
        IMMUTABLE
    } else {
        REVALIDATE
    }
}

/// Paths that belong to the API, so an unknown one gets a JSON 404 instead
/// of the app's HTML.
pub fn is_api_path(path: &str) -> bool {
    path == "/api" || path.starts_with("/api/") || path == "/healthz"
}

/// True when the last path segment has an extension (`/assets/x.js`). A
/// missing file like that is a 404; a missing extensionless path is an app
/// route (`/accounts/123`) and gets `index.html`.
fn looks_like_file(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|last| last.contains('.'))
}

/// Answers one request that no API route matched.
pub fn respond(method: &Method, path: &str, lookup: impl Fn(&str) -> Option<Asset>) -> Response {
    if is_api_path(path) {
        return ApiError::App(AppError::NotFound).into_response();
    }
    if method != Method::GET && method != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let relative = path.trim_start_matches('/');
    let relative = if relative.is_empty() {
        "index.html"
    } else {
        relative
    };
    if let Some(asset) = lookup(relative) {
        return file_response(asset, cache_control(relative));
    }
    if looks_like_file(relative) {
        return StatusCode::NOT_FOUND.into_response();
    }
    match lookup("index.html") {
        Some(index) => file_response(index, REVALIDATE),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn file_response(asset: Asset, cache: &'static str) -> Response {
    let mime = HeaderValue::from_str(&asset.mime)
        .unwrap_or(HeaderValue::from_static("application/octet-stream"));
    (
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, HeaderValue::from_static(cache)),
            (
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            ),
        ],
        asset.bytes,
    )
        .into_response()
}

/// Adds the web app as the router's fallback when built with `embed-web`.
#[cfg(feature = "embed-web")]
pub fn with_web_app<S: Clone + Send + Sync + 'static>(router: axum::Router<S>) -> axum::Router<S> {
    router.fallback(embedded::serve)
}

/// Without `embed-web` the router stays API-only.
#[cfg(not(feature = "embed-web"))]
pub fn with_web_app<S: Clone + Send + Sync + 'static>(router: axum::Router<S>) -> axum::Router<S> {
    router
}

#[cfg(feature = "embed-web")]
mod embedded {
    use axum::http::{Method, Uri};
    use axum::response::Response;
    use rust_embed::RustEmbed;

    use super::{Asset, respond};

    /// `web/dist`, read at compile time. The Vite manifest is only for the
    /// bundle size check and is left out.
    #[derive(RustEmbed)]
    #[folder = "../../web/dist/"]
    #[exclude = ".vite/*"]
    struct WebDist;

    pub async fn serve(method: Method, uri: Uri) -> Response {
        respond(&method, uri.path(), |path| {
            WebDist::get(path).map(|file| Asset {
                mime: file.metadata.mimetype().to_string(),
                bytes: file.data,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(path: &str) -> Option<Asset> {
        let (bytes, mime): (&'static [u8], &str) = match path {
            "index.html" => (b"<html>app</html>", "text/html"),
            "assets/index-abc123.js" => (b"console.log(1)", "text/javascript"),
            "sw.js" => (b"self", "text/javascript"),
            _ => return None,
        };
        Some(Asset {
            bytes: Cow::Borrowed(bytes),
            mime: mime.to_string(),
        })
    }

    fn header(response: &Response, name: header::HeaderName) -> &str {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    }

    #[test]
    fn hashed_assets_are_cached_forever_and_the_rest_revalidated() {
        assert_eq!(cache_control("assets/index-abc123.js"), IMMUTABLE);
        assert_eq!(cache_control("index.html"), REVALIDATE);
        assert_eq!(cache_control("sw.js"), REVALIDATE);
        assert_eq!(cache_control("manifest.webmanifest"), REVALIDATE);
    }

    #[test]
    fn serves_files_with_their_type_and_cache_header() {
        let asset = respond(&Method::GET, "/assets/index-abc123.js", lookup);
        assert_eq!(asset.status(), StatusCode::OK);
        assert_eq!(header(&asset, header::CONTENT_TYPE), "text/javascript");
        assert_eq!(header(&asset, header::CACHE_CONTROL), IMMUTABLE);
        let worker = respond(&Method::GET, "/sw.js", lookup);
        assert_eq!(header(&worker, header::CACHE_CONTROL), REVALIDATE);
    }

    #[test]
    fn app_routes_fall_back_to_index_html() {
        for path in ["/", "/accounts", "/transactions/0190f5c3-7a2b/edit"] {
            let response = respond(&Method::GET, path, lookup);
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert_eq!(header(&response, header::CONTENT_TYPE), "text/html");
            assert_eq!(header(&response, header::CACHE_CONTROL), REVALIDATE);
        }
    }

    #[test]
    fn missing_files_and_api_paths_are_404() {
        let missing = respond(&Method::GET, "/assets/old-hash.js", lookup);
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        let api = respond(&Method::GET, "/api/v1/nope", lookup);
        assert_eq!(api.status(), StatusCode::NOT_FOUND);
        assert_eq!(header(&api, header::CONTENT_TYPE), "application/json");
        assert!(is_api_path("/healthz"));
        assert!(is_api_path("/api"));
        assert!(!is_api_path("/apiary"));
    }

    #[test]
    fn writes_outside_the_api_are_rejected() {
        let response = respond(&Method::POST, "/accounts", lookup);
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[test]
    fn without_a_build_every_page_is_404() {
        let response = respond(&Method::GET, "/accounts", |_| None);
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
