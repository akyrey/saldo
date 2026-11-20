//! Logging in with an external identity provider (Authentik over OpenID
//! Connect): the provider vouches for the person; Saldo maps them to a member.

#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use saldo_app::error::AppError;
use saldo_app::identity::{ExternalIdentity, SessionClient};

const ISSUER: &str = "https://auth.example.com/application/o/saldo/";

fn identity(subject: &str, email: &str, verified: bool) -> ExternalIdentity {
    ExternalIdentity {
        issuer: ISSUER.to_string(),
        subject: subject.to_string(),
        email: Some(email.to_string()),
        email_verified: verified,
    }
}

#[tokio::test]
async fn a_verified_email_of_a_member_logs_in_and_links_the_identity() {
    let (test, setup) = with_household().await;

    let first = test
        .app
        .identity
        .login_external(
            identity("sub-1", "Owner@Example.com", true),
            SessionClient::Browser,
        )
        .await
        .unwrap();

    assert_eq!(first.ctx.user_id, setup.ctx.user_id);
    assert_eq!(first.ctx.household_id, setup.ctx.household_id);
    let me = test.app.identity.authenticate(&first.token).await.unwrap();
    assert_eq!(me.ctx.user_id, setup.ctx.user_id);

    // Once linked, the subject alone finds the member, even after the email
    // changed in the provider.
    let again = test
        .app
        .identity
        .login_external(
            identity("sub-1", "renamed@example.com", false),
            SessionClient::Browser,
        )
        .await
        .unwrap();
    assert_eq!(again.ctx.user_id, setup.ctx.user_id);
}

#[tokio::test]
async fn unknown_or_unverified_emails_are_refused() {
    let (test, _) = with_household().await;
    let identity_service = &test.app.identity;

    let stranger = identity_service
        .login_external(
            identity("sub-2", "stranger@example.com", true),
            SessionClient::Browser,
        )
        .await;
    assert!(matches!(stranger, Err(AppError::Unauthorized)));

    let unverified = identity_service
        .login_external(
            identity("sub-3", "owner@example.com", false),
            SessionClient::Browser,
        )
        .await;
    assert!(matches!(unverified, Err(AppError::Unauthorized)));

    let no_email = identity_service
        .login_external(
            ExternalIdentity {
                email: None,
                ..identity("sub-4", "", true)
            },
            SessionClient::Browser,
        )
        .await;
    assert!(matches!(no_email, Err(AppError::Unauthorized)));
}

#[tokio::test]
async fn a_removed_member_cannot_log_in_with_a_linked_identity() {
    let (test, setup) = with_household().await;
    let member = add_member(&test, &setup.ctx, "partner@example.com").await;
    test.app
        .identity
        .login_external(
            identity("sub-5", "partner@example.com", true),
            SessionClient::Browser,
        )
        .await
        .unwrap();
    test.app
        .identity
        .remove_member(&setup.ctx, saldo_app::ctx::Origin::Api, member.user_id)
        .await
        .unwrap();

    let after = test
        .app
        .identity
        .login_external(
            identity("sub-5", "partner@example.com", true),
            SessionClient::Browser,
        )
        .await;
    assert!(matches!(after, Err(AppError::Unauthorized)));
}
