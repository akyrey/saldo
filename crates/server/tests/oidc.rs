//! "Log in with Authentik" against a fake provider: wiremock serves the
//! discovery document, the signing keys and the token endpoint, and the ID
//! tokens are signed with a test-only RSA key (tests/fixtures).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use chrono::{Duration, Utc};
use common::*;
use openidconnect::core::{
    CoreIdToken, CoreIdTokenClaims, CoreJsonWebKeySet, CoreJwsSigningAlgorithm,
    CoreRsaPrivateSigningKey,
};
use openidconnect::{
    Audience, EmptyAdditionalClaims, EndUserEmail, IssuerUrl, JsonWebKeyId, Nonce,
    PrivateSigningKey, StandardClaims, SubjectIdentifier,
};
use serde_json::{Value, json};
use tower::ServiceExt;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = include_str!("fixtures/oidc-test-key.pem");
const CLIENT_ID: &str = "saldo";

fn key() -> CoreRsaPrivateSigningKey {
    CoreRsaPrivateSigningKey::from_pem(KEY, Some(JsonWebKeyId::new("k1".into()))).unwrap()
}

/// A fake Authentik with discovery and keys; the token answer comes later.
async fn provider() -> (MockServer, String) {
    let mock = MockServer::start().await;
    let issuer = format!("{}/application/o/saldo/", mock.uri());
    let discovery = json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{}/authorize", mock.uri()),
        "token_endpoint": format!("{}/token", mock.uri()),
        "jwks_uri": format!("{}/jwks", mock.uri()),
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
    });
    Mock::given(method("GET"))
        .and(path(
            "/application/o/saldo/.well-known/openid-configuration",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(discovery))
        .mount(&mock)
        .await;
    let jwks = CoreJsonWebKeySet::new(vec![key().as_verification_key()]);
    Mock::given(method("GET"))
        .and(path("/jwks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(jwks))
        .mount(&mock)
        .await;
    (mock, issuer)
}

fn id_token(issuer: &str, subject: &str, email: &str, nonce: &str) -> String {
    id_token_verified(issuer, subject, email, nonce, true)
}

fn id_token_verified(
    issuer: &str,
    subject: &str,
    email: &str,
    nonce: &str,
    verified: bool,
) -> String {
    let standard = StandardClaims::new(SubjectIdentifier::new(subject.into()))
        .set_email(Some(EndUserEmail::new(email.into())))
        .set_email_verified(Some(verified));
    let claims = CoreIdTokenClaims::new(
        IssuerUrl::new(issuer.into()).unwrap(),
        vec![Audience::new(CLIENT_ID.into())],
        Utc::now() + Duration::minutes(5),
        Utc::now(),
        standard,
        EmptyAdditionalClaims {},
    )
    .set_nonce(Some(Nonce::new(nonce.into())));
    let token = CoreIdToken::new(
        claims,
        &key(),
        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
        None,
        None,
    )
    .unwrap();
    token.to_string()
}

async fn answer_tokens(mock: &MockServer, id_token: String) {
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("code_verifier="))
        .and(body_string_contains("code=the-code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "at",
            "token_type": "Bearer",
            "expires_in": 300,
            "id_token": id_token,
        })))
        .mount(mock)
        .await;
}

async fn saldo_with(issuer: &str, extra: &[(&'static str, &'static str)]) -> TestServer {
    let issuer: &'static str = Box::leak(issuer.to_string().into_boxed_str());
    let mut env = vec![
        ("SALDO_OIDC_ISSUER", issuer),
        ("SALDO_OIDC_CLIENT_ID", CLIENT_ID),
        ("SALDO_OIDC_CLIENT_SECRET", "client-secret"),
    ];
    env.extend_from_slice(extra);
    let server = server_with_env(&env).await;
    let setup = server
        .post(
            "/api/v1/setup",
            setup_body("owner@example.com"),
            &Auth::None,
        )
        .await;
    assert_eq!(setup.status, StatusCode::CREATED);
    server
}

struct Raw {
    status: StatusCode,
    location: String,
    cookies: Vec<String>,
}

async fn get_raw(server: &TestServer, uri: &str, cookie: Option<&str>) -> Raw {
    let mut request = Request::builder().uri(uri);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    let response = server
        .router
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let headers = response.headers();
    Raw {
        status: response.status(),
        location: headers
            .get(header::LOCATION)
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default(),
        cookies: headers
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|v| v.to_str().unwrap().to_string())
            .collect(),
    }
}

fn query_param(url: &str, name: &str) -> String {
    let url = url::Url::parse(url).unwrap();
    url.query_pairs()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default()
}

/// Starts a login; returns the provider URL and the browser's state cookie.
async fn start_login(server: &TestServer) -> (String, String) {
    let start = get_raw(server, "/api/v1/auth/oidc/start?redirect=/budgets", None).await;
    assert_eq!(start.status, StatusCode::SEE_OTHER, "{}", start.location);
    let cookie = start.cookies[0].split(';').next().unwrap().to_string();
    (start.location, cookie)
}

#[tokio::test]
async fn logs_a_member_in_through_the_provider() {
    let (mock, issuer) = provider().await;
    let server = saldo_with(&issuer, &[]).await;

    let (url, cookie) = start_login(&server).await;
    assert!(url.starts_with(&format!("{}/authorize", mock.uri())));
    assert_eq!(query_param(&url, "client_id"), CLIENT_ID);
    assert_eq!(query_param(&url, "code_challenge_method"), "S256");
    assert_eq!(
        query_param(&url, "redirect_uri"),
        "http://localhost:8080/api/v1/auth/oidc/callback"
    );
    let state = query_param(&url, "state");
    let nonce = query_param(&url, "nonce");
    answer_tokens(
        &mock,
        id_token(&issuer, "ak-1", "Owner@Example.com", &nonce),
    )
    .await;

    let back = get_raw(
        &server,
        &format!("/api/v1/auth/oidc/callback?code=the-code&state={state}"),
        Some(&cookie),
    )
    .await;

    assert_eq!(back.status, StatusCode::SEE_OTHER);
    assert_eq!(back.location, "/budgets");
    let session = back
        .cookies
        .iter()
        .find_map(|c| c.strip_prefix("saldo_session="))
        .map(|c| c.split(';').next().unwrap().to_string())
        .unwrap();
    let me = server.get("/api/v1/auth/me", &Auth::Cookie(session)).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["data"]["user"]["email"], "owner@example.com");
}

#[tokio::test]
async fn a_callback_from_another_browser_or_a_reused_state_fails() {
    let (mock, issuer) = provider().await;
    let server = saldo_with(&issuer, &[]).await;
    let (url, cookie) = start_login(&server).await;
    let state = query_param(&url, "state");
    answer_tokens(
        &mock,
        id_token(
            &issuer,
            "ak-1",
            "owner@example.com",
            &query_param(&url, "nonce"),
        ),
    )
    .await;
    let callback = format!("/api/v1/auth/oidc/callback?code=the-code&state={state}");

    let other_browser = get_raw(&server, &callback, Some("saldo_oidc=someone-else")).await;
    assert_eq!(other_browser.location, "/login?error=oidc_expired");
    // The state was used up by the failed attempt too.
    let replay = get_raw(&server, &callback, Some(&cookie)).await;
    assert_eq!(replay.location, "/login?error=oidc_expired");
}

/// Starts a login, lets the provider answer with this ID token, and returns
/// where the callback sends the browser.
async fn login_with(subject: &str, email: &str, nonce: impl Fn(&str) -> String) -> String {
    let (mock, issuer) = provider().await;
    let server = saldo_with(&issuer, &[]).await;
    let (url, cookie) = start_login(&server).await;
    let token = id_token(&issuer, subject, email, &nonce(&query_param(&url, "nonce")));
    answer_tokens(&mock, token).await;
    let state = query_param(&url, "state");
    get_raw(
        &server,
        &format!("/api/v1/auth/oidc/callback?code=the-code&state={state}"),
        Some(&cookie),
    )
    .await
    .location
}

#[tokio::test]
async fn people_who_are_not_members_are_refused() {
    let location = login_with("ak-9", "stranger@example.com", |n| n.to_string()).await;
    assert_eq!(location, "/login?error=oidc_no_account");
}

#[tokio::test]
async fn an_id_token_for_another_login_is_refused() {
    let location = login_with("ak-1", "owner@example.com", |_| "wrong-nonce".to_string()).await;
    assert_eq!(location, "/login?error=oidc_invalid");
}

/// Logs in with an unverified email; returns where the browser lands.
async fn login_unverified(extra: &[(&'static str, &'static str)]) -> String {
    let (mock, issuer) = provider().await;
    let server = saldo_with(&issuer, extra).await;
    let (url, cookie) = start_login(&server).await;
    let nonce = query_param(&url, "nonce");
    answer_tokens(
        &mock,
        id_token_verified(&issuer, "ak-1", "owner@example.com", &nonce, false),
    )
    .await;
    let state = query_param(&url, "state");
    get_raw(
        &server,
        &format!("/api/v1/auth/oidc/callback?code=the-code&state={state}"),
        Some(&cookie),
    )
    .await
    .location
}

#[tokio::test]
async fn unverified_emails_link_only_when_the_provider_is_trusted() {
    assert_eq!(login_unverified(&[]).await, "/login?error=oidc_no_account");
    assert_eq!(
        login_unverified(&[("SALDO_OIDC_TRUST_EMAIL", "true")]).await,
        "/budgets"
    );
}

#[tokio::test]
async fn the_provider_saying_no_returns_to_the_login_page() {
    let (_mock, issuer) = provider().await;
    let server = saldo_with(&issuer, &[]).await;
    let refused = get_raw(
        &server,
        "/api/v1/auth/oidc/callback?error=access_denied&state=x",
        None,
    )
    .await;
    assert_eq!(refused.location, "/login?error=oidc_refused");
}

#[tokio::test]
async fn password_login_can_be_turned_off() {
    let (_mock, issuer) = provider().await;
    let server = saldo_with(&issuer, &[("SALDO_PASSWORD_LOGIN", "false")]).await;

    let methods = server.get("/api/v1/auth/methods", &Auth::None).await;
    assert_eq!(
        methods.body["data"],
        json!({ "password": false, "oidc": { "name": "Authentik" } })
    );
    let login = server
        .post(
            "/api/v1/auth/login",
            json!({ "email": "owner@example.com", "password": PASSWORD }),
            &Auth::None,
        )
        .await;
    assert_eq!(login.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn without_a_provider_only_passwords_are_offered() {
    let server = server().await;
    let methods = server.get("/api/v1/auth/methods", &Auth::None).await;
    assert_eq!(
        methods.body["data"],
        json!({ "password": true, "oidc": Value::Null })
    );
    let start = get_raw(&server, "/api/v1/auth/oidc/start", None).await;
    assert_eq!(start.location, "/login?error=oidc_disabled");
}

// --- Authentik's default certificate: ECDSA P-384, tokens labelled ES256 ---

mod authentik_ec {
    use base64ct::{Base64UrlUnpadded, Encoding};
    use p384::ecdsa::signature::hazmat::PrehashSigner;
    use p384::ecdsa::{Signature, SigningKey};
    use sha2::{Digest, Sha256};

    use super::*;

    fn b64(bytes: &[u8]) -> String {
        Base64UrlUnpadded::encode_string(bytes)
    }

    fn key() -> SigningKey {
        SigningKey::from_slice(&[11u8; 48]).unwrap()
    }

    /// A fake Authentik publishing a P-384 key and announcing ES256.
    async fn provider() -> (MockServer, String) {
        let mock = MockServer::start().await;
        let issuer = format!("{}/application/o/saldo/", mock.uri());
        let discovery = json!({
            "issuer": issuer,
            "authorization_endpoint": format!("{}/authorize", mock.uri()),
            "token_endpoint": format!("{}/token", mock.uri()),
            "jwks_uri": format!("{}/jwks", mock.uri()),
            "response_types_supported": ["code"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["ES256"],
        });
        Mock::given(method("GET"))
            .and(path(
                "/application/o/saldo/.well-known/openid-configuration",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(discovery))
            .mount(&mock)
            .await;
        let point = key().verifying_key().to_sec1_bytes();
        let jwks = json!({ "keys": [{
            "kty": "EC", "crv": "P-384", "alg": "ES256", "use": "sig", "kid": "ec1",
            "x": b64(&point[1..49]), "y": b64(&point[49..97]),
        }]});
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(jwks))
            .mount(&mock)
            .await;
        (mock, issuer)
    }

    /// Signed like Authentik: P-384 key, SHA-256 hash, header `ES256`.
    fn id_token(issuer: &str, audience: &str, nonce: &str) -> String {
        let now = Utc::now().timestamp();
        let header = b64(json!({ "alg": "ES256", "kid": "ec1", "typ": "JWT" })
            .to_string()
            .as_bytes());
        let claims = json!({
            "iss": issuer, "aud": audience, "exp": now + 300, "iat": now,
            "sub": "ak-1", "email": "owner@example.com", "email_verified": true, "nonce": nonce,
        });
        let signed = format!("{header}.{}", b64(claims.to_string().as_bytes()));
        let signature: Signature = key()
            .sign_prehash(&Sha256::digest(signed.as_bytes()))
            .unwrap();
        format!("{signed}.{}", b64(&signature.to_bytes()))
    }

    async fn login(audience: &str) -> String {
        let (mock, issuer) = provider().await;
        let server = saldo_with(&issuer, &[]).await;
        let (url, cookie) = start_login(&server).await;
        answer_tokens(
            &mock,
            id_token(&issuer, audience, &query_param(&url, "nonce")),
        )
        .await;
        let state = query_param(&url, "state");
        get_raw(
            &server,
            &format!("/api/v1/auth/oidc/callback?code=the-code&state={state}"),
            Some(&cookie),
        )
        .await
        .location
    }

    #[tokio::test]
    async fn logs_in_with_authentiks_default_ec_certificate() {
        assert_eq!(login(CLIENT_ID).await, "/budgets");
    }

    #[tokio::test]
    async fn still_checks_the_audience_of_such_tokens() {
        assert_eq!(login("another-app").await, "/login?error=oidc_invalid");
    }
}

// --- The desktop app: system browser, then back over a loopback port ---

mod desktop {
    use base64ct::{Base64UrlUnpadded, Encoding};
    use sha2::{Digest, Sha256};

    use super::*;

    const VERIFIER: &str = "a-random-verifier-made-by-the-desktop-app-0123456789";

    fn challenge(verifier: &str) -> String {
        Base64UrlUnpadded::encode_string(&Sha256::digest(verifier.as_bytes()))
    }

    /// Runs a desktop login for `email` up to the redirect to the app's port.
    async fn to_loopback(email: &str) -> (TestServer, String) {
        let (mock, issuer) = provider().await;
        let server = saldo_with(&issuer, &[]).await;
        let start = get_raw(
            &server,
            &format!(
                "/api/v1/auth/oidc/start?desktop_port=53682&desktop_challenge={}",
                challenge(VERIFIER)
            ),
            None,
        )
        .await;
        assert_eq!(start.status, StatusCode::SEE_OTHER, "{}", start.location);
        let cookie = start.cookies[0].split(';').next().unwrap().to_string();
        let nonce = query_param(&start.location, "nonce");
        answer_tokens(&mock, id_token(&issuer, "ak-1", email, &nonce)).await;
        let state = query_param(&start.location, "state");
        let back = get_raw(
            &server,
            &format!("/api/v1/auth/oidc/callback?code=the-code&state={state}"),
            Some(&cookie),
        )
        .await;
        // The token never travels in a URL, and the browser gets no session.
        assert!(
            back.cookies
                .iter()
                .all(|c| !c.starts_with("saldo_session="))
        );
        (server, back.location)
    }

    async fn exchange(server: &TestServer, code: &str, verifier: &str) -> Response {
        server
            .post(
                "/api/v1/auth/oidc/desktop-token",
                json!({ "code": code, "verifier": verifier }),
                &Auth::None,
            )
            .await
    }

    #[tokio::test]
    async fn hands_the_token_to_the_app_for_its_verifier_once() {
        let (server, location) = to_loopback("owner@example.com").await;
        assert!(
            location.starts_with("http://127.0.0.1:53682/callback?code="),
            "{location}"
        );
        let code = query_param(&location, "code");

        let wrong = exchange(&server, &code, "someone-elses-verifier").await;
        assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
        // A wrong verifier used the code up: an attacker gets one guess at most.
        let after_wrong = exchange(&server, &code, VERIFIER).await;
        assert_eq!(after_wrong.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn the_right_verifier_gets_a_working_token() {
        let (server, location) = to_loopback("owner@example.com").await;
        let code = query_param(&location, "code");

        let ok = exchange(&server, &code, VERIFIER).await;
        assert_eq!(ok.status, StatusCode::OK, "{}", ok.body);
        let token = ok.body["data"]["token"].as_str().unwrap().to_string();
        let me = server.get("/api/v1/auth/me", &Auth::Bearer(token)).await;
        assert_eq!(me.status, StatusCode::OK);
        let again = exchange(&server, &code, VERIFIER).await;
        assert_eq!(again.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn failures_go_back_to_the_app_too() {
        let (_server, location) = to_loopback("stranger@example.com").await;
        assert_eq!(
            location,
            "http://127.0.0.1:53682/callback?error=oidc_no_account"
        );
    }

    #[tokio::test]
    async fn bad_desktop_parameters_are_refused() {
        let (_mock, issuer) = provider().await;
        let server = saldo_with(&issuer, &[]).await;
        for query in [
            "desktop_port=80&desktop_challenge=abc",
            "desktop_port=53682",
            "desktop_port=53682&desktop_challenge=not%20base64url",
        ] {
            let start = get_raw(&server, &format!("/api/v1/auth/oidc/start?{query}"), None).await;
            assert_eq!(start.location, "/login?error=oidc_bad_request", "{query}");
        }
    }
}

mod desktop_cancel {
    use super::*;

    #[tokio::test]
    async fn a_cancelled_desktop_login_tells_the_app() {
        let (_mock, issuer) = provider().await;
        let server = saldo_with(&issuer, &[]).await;
        let challenge = "x".repeat(43);
        let start = get_raw(
            &server,
            &format!("/api/v1/auth/oidc/start?desktop_port=53682&desktop_challenge={challenge}"),
            None,
        )
        .await;
        let cookie = start.cookies[0].split(';').next().unwrap().to_string();
        let state = query_param(&start.location, "state");
        let back = get_raw(
            &server,
            &format!("/api/v1/auth/oidc/callback?error=access_denied&state={state}"),
            Some(&cookie),
        )
        .await;
        assert_eq!(
            back.location,
            "http://127.0.0.1:53682/callback?error=oidc_refused"
        );
    }
}
