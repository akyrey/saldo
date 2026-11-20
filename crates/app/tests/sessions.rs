#![allow(clippy::unwrap_used)] // helpers outside #[test] functions

mod common;

use chrono::TimeDelta;
use common::*;
use saldo_app::error::AppError;
use saldo_app::identity::{Authenticated, SessionClient};

/// Logs in and returns what `authenticate` resolves the new token to.
async fn log_in(test: &TestApp, email: &str, client: SessionClient) -> (String, Authenticated) {
    let login = test
        .app
        .identity
        .login(email, PASSWORD, client)
        .await
        .unwrap();
    let auth = test.app.identity.authenticate(&login.token).await.unwrap();
    (login.token, auth)
}

#[tokio::test]
async fn sessions_list_the_callers_live_logins_newest_first() {
    let (test, setup) = with_household().await;
    let (_, browser) = log_in(&test, "owner@example.com", SessionClient::Browser).await;
    test.clock.advance(TimeDelta::minutes(1));
    let (_, desktop) = log_in(&test, "owner@example.com", SessionClient::Desktop).await;
    // Not listed: a token, another member's session, an expired session.
    test.app
        .identity
        .create_token(&setup.ctx, "script")
        .await
        .unwrap();
    add_member(&test, &setup.ctx, "member@example.com").await;

    let sessions = test
        .app
        .identity
        .list_sessions(&browser.ctx, &browser.session_id)
        .await
        .unwrap();

    let ids: Vec<&str> = sessions.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        [desktop.session_id.as_str(), browser.session_id.as_str()]
    );
    assert_eq!(sessions[0].client, Some(SessionClient::Desktop));
    assert_eq!(sessions[1].client, Some(SessionClient::Browser));
    assert!(!sessions[0].current);
    assert!(sessions[1].current);
    assert_eq!(sessions[1].created_at, "2026-11-13T09:00:00.000Z");
    assert_eq!(sessions[1].expires_at, "2026-12-13T09:00:00.000Z");
    assert!(sessions[1].last_used_at.is_some());
}

#[tokio::test]
async fn expired_sessions_are_not_listed() {
    let (test, _) = with_household().await;
    let (_, old) = log_in(&test, "owner@example.com", SessionClient::Browser).await;
    test.clock.advance(TimeDelta::days(29));
    let (_, new) = log_in(&test, "owner@example.com", SessionClient::Browser).await;
    test.clock.advance(TimeDelta::days(2));

    let sessions = test
        .app
        .identity
        .list_sessions(&new.ctx, &new.session_id)
        .await
        .unwrap();

    assert_eq!(sessions.len(), 1);
    assert_ne!(sessions[0].id, old.session_id);
}

#[tokio::test]
async fn ending_a_session_logs_that_device_out() {
    let (test, _) = with_household().await;
    let (_, here) = log_in(&test, "owner@example.com", SessionClient::Browser).await;
    let (phone_token, phone) = log_in(&test, "owner@example.com", SessionClient::Browser).await;

    test.app
        .identity
        .end_session(&here.ctx, &phone.session_id)
        .await
        .unwrap();

    assert!(matches!(
        test.app.identity.authenticate(&phone_token).await,
        Err(AppError::Unauthorized)
    ));
    let left = test
        .app
        .identity
        .list_sessions(&here.ctx, &here.session_id)
        .await
        .unwrap();
    assert_eq!(left.len(), 1);
}

#[tokio::test]
async fn only_the_callers_own_login_sessions_can_be_ended() {
    let (test, setup) = with_household().await;
    let (_, owner) = log_in(&test, "owner@example.com", SessionClient::Browser).await;
    let member_ctx = add_member(&test, &setup.ctx, "member@example.com").await;
    let (member_token, member) = log_in(&test, "member@example.com", SessionClient::Browser).await;
    let token = test
        .app
        .identity
        .create_token(&owner.ctx, "script")
        .await
        .unwrap();

    let others = test
        .app
        .identity
        .end_session(&owner.ctx, &member.session_id)
        .await;
    let a_token = test
        .app
        .identity
        .end_session(&owner.ctx, &token.view.id)
        .await;

    assert!(matches!(others, Err(AppError::NotFound)));
    assert!(matches!(a_token, Err(AppError::NotFound)));
    assert!(test.app.identity.authenticate(&member_token).await.is_ok());
    assert!(test.app.identity.authenticate(&token.token).await.is_ok());
    assert_eq!(member.ctx, member_ctx);
}

#[tokio::test]
async fn ending_the_other_sessions_keeps_this_one_and_the_tokens() {
    let (test, setup) = with_household().await;
    let (here_token, here) = log_in(&test, "owner@example.com", SessionClient::Browser).await;
    let (laptop, _) = log_in(&test, "owner@example.com", SessionClient::Browser).await;
    let (desktop, _) = log_in(&test, "owner@example.com", SessionClient::Desktop).await;
    let token = test
        .app
        .identity
        .create_token(&setup.ctx, "script")
        .await
        .unwrap();
    add_member(&test, &setup.ctx, "member@example.com").await;
    let (member_token, _) = log_in(&test, "member@example.com", SessionClient::Browser).await;

    let ended = test
        .app
        .identity
        .end_other_sessions(&here.ctx, &here.session_id)
        .await
        .unwrap();

    assert_eq!(ended, 2);
    assert!(test.app.identity.authenticate(&here_token).await.is_ok());
    assert!(test.app.identity.authenticate(&laptop).await.is_err());
    assert!(test.app.identity.authenticate(&desktop).await.is_err());
    assert!(test.app.identity.authenticate(&token.token).await.is_ok());
    assert!(test.app.identity.authenticate(&member_token).await.is_ok());
}
