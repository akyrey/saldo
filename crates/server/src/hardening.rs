//! Protection that wraps every request: browser security headers and a
//! request timeout. Like a Laravel global middleware group.

use std::time::Duration;

use axum::Router;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::middleware;
use axum::response::Response;
use tower_http::timeout::TimeoutLayer;

/// What the web app may load. Everything comes from our own origin; images
/// may also be `data:` or `blob:` URLs (icons, attachment previews), and
/// styles may be inline because UI libraries set `style` attributes. No
/// inline scripts, so an injected `<script>` never runs.
pub const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; \
script-src 'self'; \
style-src 'self' 'unsafe-inline'; \
img-src 'self' data: blob:; \
font-src 'self' data:; \
connect-src 'self'; \
worker-src 'self'; \
manifest-src 'self'; \
object-src 'none'; \
base-uri 'self'; \
form-action 'self'; \
frame-ancestors 'none'";

/// One year: browsers only use https for this host from now on.
const HSTS: &str = "max-age=31536000";

/// How long one request may take before it is answered with 408. Long
/// enough for a 20 MB attachment on a slow phone connection. The live
/// update stream is not affected: its response starts at once, and only
/// the wait for the response (not its body) is timed.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Adds the security headers and the timeout to `router`. `https` is true
/// when the public URL is https, which turns on HSTS.
pub fn protect<S: Clone + Send + Sync + 'static>(router: Router<S>, https: bool) -> Router<S> {
    router
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        .layer(middleware::map_response(
            move |response: Response| async move { with_security_headers(response, https) },
        ))
}

/// The response with every security header it does not set itself.
fn with_security_headers(mut response: Response, https: bool) -> Response {
    let headers = response.headers_mut();
    set_default(
        headers,
        header::CONTENT_SECURITY_POLICY,
        CONTENT_SECURITY_POLICY,
    );
    set_default(headers, header::X_FRAME_OPTIONS, "DENY");
    set_default(headers, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    set_default(headers, header::REFERRER_POLICY, "no-referrer");
    if https {
        set_default(headers, header::STRICT_TRANSPORT_SECURITY, HSTS);
    }
    response
}

fn set_default(headers: &mut HeaderMap, name: HeaderName, value: &'static str) {
    if !headers.contains_key(&name) {
        headers.insert(name, HeaderValue::from_static(value));
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::get;
    use tower::ServiceExt;

    use super::*;

    async fn status_after(handler_delay: Duration) -> StatusCode {
        let router = Router::new().route(
            "/slow",
            get(move || async move {
                tokio::time::sleep(handler_delay).await;
                "done"
            }),
        );
        let router = protect(router, false);
        let request = Request::get("/slow").body(Body::empty()).unwrap();
        router.oneshot(request).await.unwrap().status()
    }

    // `start_paused` runs tokio's clock by hand, so two minutes pass at once.
    #[tokio::test(start_paused = true)]
    async fn a_request_slower_than_the_timeout_gets_408() {
        let late = REQUEST_TIMEOUT + Duration::from_secs(1);
        assert_eq!(status_after(late).await, StatusCode::REQUEST_TIMEOUT);
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_within_the_timeout_is_answered() {
        let early = REQUEST_TIMEOUT - Duration::from_secs(1);
        assert_eq!(status_after(early).await, StatusCode::OK);
    }

    #[test]
    fn headers_already_set_by_a_route_are_kept() {
        let response = Response::builder()
            .header(header::X_FRAME_OPTIONS, "SAMEORIGIN")
            .body(axum::body::Body::empty())
            .unwrap();
        let response = with_security_headers(response, true);
        assert_eq!(response.headers()[header::X_FRAME_OPTIONS], "SAMEORIGIN");
        assert_eq!(response.headers()[header::STRICT_TRANSPORT_SECURITY], HSTS);
    }
}
