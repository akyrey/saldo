mod common;

use axum::http::StatusCode;
use common::*;

fn header<'a>(response: &'a Response, name: &str) -> Option<&'a str> {
    response.headers.get(name).and_then(|v| v.to_str().ok())
}

#[tokio::test]
async fn every_response_carries_the_security_headers() {
    let server = server().await;
    // An API success, an API error and a non-API path all get them.
    for path in ["/healthz", "/api/v1/accounts", "/accounts"] {
        let response = server.get(path, &Auth::None).await;
        let csp = header(&response, "content-security-policy").unwrap_or_default();
        assert!(csp.contains("default-src 'self'"), "{path}: {csp}");
        assert!(csp.contains("script-src 'self'"), "{path}: {csp}");
        assert!(csp.contains("frame-ancestors 'none'"), "{path}: {csp}");
        assert!(csp.contains("object-src 'none'"), "{path}: {csp}");
        assert_eq!(header(&response, "x-frame-options"), Some("DENY"), "{path}");
        assert_eq!(
            header(&response, "x-content-type-options"),
            Some("nosniff"),
            "{path}"
        );
        assert_eq!(
            header(&response, "referrer-policy"),
            Some("no-referrer"),
            "{path}"
        );
    }
}

#[tokio::test]
async fn hsts_is_sent_only_when_the_public_url_is_https() {
    let plain = server().await;
    let response = plain.get("/healthz", &Auth::None).await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(header(&response, "strict-transport-security"), None);

    let tls = server_with_env(&[("SALDO_PUBLIC_URL", "https://money.example.com")]).await;
    let response = tls.get("/healthz", &Auth::None).await;
    assert_eq!(
        header(&response, "strict-transport-security"),
        Some("max-age=31536000")
    );
}
