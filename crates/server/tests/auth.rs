mod common;

use axum::http::{Method, StatusCode, header};
use common::*;
use serde_json::json;

#[tokio::test]
async fn setup_works_once_and_validates() {
    let server = server().await;
    let invalid = server.post("/api/v1/setup", json!({ "email": "x", "password": "short", "display_name": "O", "household_name": "H", "primary_currency": "EUR" }), &Auth::None).await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(invalid.body["error"]["code"], "validation_failed");
    assert_eq!(invalid.body["error"]["details"][0]["field"], "email");

    let created = server
        .post(
            "/api/v1/setup",
            setup_body("owner@example.com"),
            &Auth::None,
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED);
    assert_eq!(created.body["data"]["user"]["email"], "owner@example.com");
    assert_eq!(created.body["data"]["household"]["primary_currency"], "EUR");

    let again = server
        .post(
            "/api/v1/setup",
            setup_body("other@example.com"),
            &Auth::None,
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(again.body["error"]["code"], "conflict");
}

#[tokio::test]
async fn malformed_json_is_a_bad_request_in_the_envelope() {
    let server = server().await;
    let response = server
        .post("/api/v1/setup", json!({ "email": 3 }), &Auth::None)
        .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    assert_eq!(response.body["error"]["code"], "bad_request");
}

#[tokio::test]
async fn login_sets_a_secure_style_cookie_and_wrong_passwords_fail() {
    let server = server().await;
    server
        .post(
            "/api/v1/setup",
            setup_body("owner@example.com"),
            &Auth::None,
        )
        .await;

    let wrong = server
        .post(
            "/api/v1/auth/login",
            json!({ "email": "owner@example.com", "password": "nope nope" }),
            &Auth::None,
        )
        .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.body["error"]["code"], "unauthorized");
    assert!(session_cookie(&wrong.headers).is_none());

    let ok = server
        .post(
            "/api/v1/auth/login",
            json!({ "email": "owner@example.com", "password": PASSWORD }),
            &Auth::None,
        )
        .await;
    assert_eq!(ok.status, StatusCode::OK);
    assert_eq!(ok.body["data"]["role"], "owner");
    let cookie = ok
        .headers
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax") && cookie.contains("Path=/")
    );
    assert!(
        !ok.body
            .to_string()
            .contains(&session_cookie(&ok.headers).unwrap()),
        "token must not be in the body"
    );
}

#[tokio::test]
async fn cookie_reads_work_but_writes_need_csrf() {
    let server = server().await;
    let token = server.setup_and_login().await;

    let me = server
        .get("/api/v1/auth/me", &Auth::CookieWithoutCsrf(token.clone()))
        .await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["data"]["user"]["email"], "owner@example.com");
    assert_eq!(me.body["data"]["role"], "owner");

    let body = json!({ "name": "script" });
    let blocked = server
        .post(
            "/api/v1/auth/tokens",
            body.clone(),
            &Auth::CookieWithoutCsrf(token.clone()),
        )
        .await;
    assert_eq!(blocked.status, StatusCode::FORBIDDEN);
    assert_eq!(blocked.body["error"]["code"], "csrf_failed");

    let allowed = server
        .post("/api/v1/auth/tokens", body, &Auth::Cookie(token))
        .await;
    assert_eq!(allowed.status, StatusCode::CREATED);
}

#[tokio::test]
async fn bearer_tokens_need_no_csrf_and_can_be_revoked() {
    let server = server().await;
    let session = server.setup_and_login().await;
    let created = server
        .post(
            "/api/v1/auth/tokens",
            json!({ "name": "importer" }),
            &Auth::Cookie(session.clone()),
        )
        .await;
    let token = created.body["data"]["token"].as_str().unwrap().to_string();
    let id = created.body["data"]["info"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let bearer = Auth::Bearer(token.clone());
    let list = server.get("/api/v1/auth/tokens", &bearer).await;
    assert_eq!(list.body["data"][0]["name"], "importer");
    assert!(list.body["data"][0].get("token").is_none());
    let write = server
        .post("/api/v1/auth/tokens", json!({ "name": "second" }), &bearer)
        .await;
    assert_eq!(write.status, StatusCode::CREATED);

    let revoke = server
        .send(
            Method::DELETE,
            &format!("/api/v1/auth/tokens/{id}"),
            None,
            &Auth::Cookie(session),
        )
        .await;
    assert_eq!(revoke.status, StatusCode::NO_CONTENT);
    assert_eq!(
        server.get("/api/v1/auth/me", &bearer).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn logout_clears_the_cookie_and_ends_the_session() {
    let server = server().await;
    let token = server.setup_and_login().await;
    let response = server
        .send(
            Method::POST,
            "/api/v1/auth/logout",
            None,
            &Auth::Cookie(token.clone()),
        )
        .await;
    assert_eq!(response.status, StatusCode::NO_CONTENT);
    assert_eq!(session_cookie(&response.headers).as_deref(), Some(""));
    assert_eq!(
        server
            .get("/api/v1/auth/me", &Auth::Cookie(token))
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn garbage_credentials_are_unauthorized() {
    let server = server().await;
    server.setup_and_login().await;
    for auth in [
        Auth::None,
        Auth::Bearer("nope".into()),
        Auth::CookieWithoutCsrf("nope".into()),
    ] {
        let response = server.get("/api/v1/auth/me", &auth).await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED);
        assert_eq!(response.body["error"]["code"], "unauthorized");
    }
}

#[tokio::test]
async fn login_is_rate_limited_per_email() {
    let server = server().await;
    server
        .post(
            "/api/v1/setup",
            setup_body("owner@example.com"),
            &Auth::None,
        )
        .await;
    let attempt = |password: &str| {
        server.post(
            "/api/v1/auth/login",
            json!({ "email": "Owner@Example.com", "password": password }),
            &Auth::None,
        )
    };
    for _ in 0..10 {
        assert_eq!(
            attempt("wrong password").await.status,
            StatusCode::UNAUTHORIZED
        );
    }
    let limited = attempt(PASSWORD).await;
    assert_eq!(limited.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(limited.body["error"]["code"], "rate_limited");
}

#[tokio::test]
async fn owner_manages_members_and_settings() {
    let server = server().await;
    let owner = Auth::Cookie(server.setup_and_login().await);

    let patched = server
        .send(
            Method::PATCH,
            "/api/v1/household",
            Some(json!({ "name": "Our place" })),
            &owner,
        )
        .await;
    assert_eq!(
        (patched.status, patched.body["data"]["name"].as_str()),
        (StatusCode::OK, Some("Our place"))
    );
    assert_eq!(
        server.get("/api/v1/household", &owner).await.body["data"]["name"],
        "Our place"
    );

    let member =
        json!({ "email": "partner@example.com", "display_name": "Partner", "password": PASSWORD });
    let added = server
        .post("/api/v1/household/members", member.clone(), &owner)
        .await;
    assert_eq!(added.status, StatusCode::CREATED);
    assert_eq!(added.body["data"]["role"], "member");
    assert_eq!(
        server
            .post("/api/v1/household/members", member, &owner)
            .await
            .status,
        StatusCode::CONFLICT
    );
    let members = server.get("/api/v1/household/members", &owner).await;
    assert_eq!(members.body["data"].as_array().unwrap().len(), 2);

    let partner = Auth::Cookie(server.login("partner@example.com", PASSWORD).await);
    let forbidden = server
        .send(
            Method::PATCH,
            "/api/v1/household",
            Some(json!({ "name": "Mine" })),
            &partner,
        )
        .await;
    assert_eq!(
        (forbidden.status, forbidden.body["error"]["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("forbidden"))
    );

    let user_id = added.body["data"]["user_id"].as_str().unwrap();
    let removed = server
        .send(
            Method::DELETE,
            &format!("/api/v1/household/members/{user_id}"),
            None,
            &owner,
        )
        .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    assert_eq!(
        server.get("/api/v1/auth/me", &partner).await.status,
        StatusCode::UNAUTHORIZED
    );
    let bad_id = server
        .send(
            Method::DELETE,
            "/api/v1/household/members/not-a-uuid",
            None,
            &owner,
        )
        .await;
    assert_eq!(bad_id.status, StatusCode::BAD_REQUEST);
}
