mod common;

use chrono::TimeDelta;
use common::*;
use saldo_app::ctx::{Origin, Role};
use saldo_app::error::AppError;
use saldo_app::events::EventFilter;
use saldo_app::identity::{HouseholdUpdate, NewMember};
use saldo_app::page::PageRequest;

#[tokio::test]
async fn setup_creates_owner_household_equity_accounts_and_events() {
    let (test, setup) = with_household().await;

    assert_eq!(setup.ctx.role, Role::Owner);
    assert_eq!(setup.user.email, "owner@example.com");
    assert_eq!(setup.household.name, "Home");
    assert_eq!(setup.household.primary_currency, "EUR");
    assert_eq!(count(&test, "accounts WHERE type = 'equity'").await, 2);

    let events = events(&test, &setup.ctx).await;
    let types: Vec<_> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert_eq!(
        types,
        [
            "household.created",
            "household_member.created",
            "account.created",
            "account.created"
        ]
    );
    let first = &events[0];
    assert_eq!(first.origin, "api");
    assert_eq!(first.actor_user_id.as_deref(), Some(setup.user.id.as_str()));
    assert_eq!(first.payload["type"], "household.created");
    assert_eq!(first.payload["data"]["name"], "Home");
    assert_eq!(first.payload["actor"]["user_id"], setup.user.id.as_str());
    assert_eq!(events[2].payload["data"]["equity_role"], "opening_balance");
}

#[tokio::test]
async fn setup_only_works_once() {
    let (test, _) = with_household().await;
    let again = test
        .app
        .identity
        .setup(Origin::Api, setup_input("other@example.com"))
        .await;
    assert!(matches!(again, Err(AppError::Conflict(_))));
    assert_eq!(count(&test, "users").await, 1);
}

#[tokio::test]
async fn setup_with_unknown_currency_writes_nothing() {
    let test = test_app().await;
    let mut input = setup_input("owner@example.com");
    input.primary_currency = "XXX".to_string();
    let result = test.app.identity.setup(Origin::Api, input).await;
    assert!(
        matches!(result, Err(AppError::Invalid { ref field, .. }) if field == "primary_currency")
    );
    assert_eq!(count(&test, "users").await, 0);
    assert_eq!(count(&test, "events").await, 0);
}

#[tokio::test]
async fn setup_rejects_bad_input_before_touching_the_database() {
    let test = test_app().await;
    let mut short = setup_input("owner@example.com");
    short.password = "short".to_string();
    assert!(test.app.identity.setup(Origin::Api, short).await.is_err());
    assert!(
        test.app
            .identity
            .setup(Origin::Api, setup_input("not-an-email"))
            .await
            .is_err()
    );
    assert_eq!(count(&test, "households").await, 0);
}

#[tokio::test]
async fn login_then_authenticate_resolves_the_context() {
    let (test, setup) = with_household().await;
    let login = test
        .app
        .identity
        .login(" owner@example.com ", PASSWORD)
        .await
        .unwrap();
    assert_eq!(login.ctx, setup.ctx);
    assert_eq!(login.token.len(), 64);
    assert_eq!(login.expires_at, "2026-12-13T09:00:00.000Z");

    let auth = test.app.identity.authenticate(&login.token).await.unwrap();
    assert_eq!(auth.ctx, setup.ctx);
    assert!(!auth.is_token);
}

#[tokio::test]
async fn login_email_is_case_insensitive() {
    let (test, _) = with_household().await;
    assert!(
        test.app
            .identity
            .login("OWNER@example.com", PASSWORD)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn wrong_password_and_unknown_email_are_unauthorized() {
    let (test, _) = with_household().await;
    let wrong = test
        .app
        .identity
        .login("owner@example.com", "wrong password")
        .await;
    let unknown = test
        .app
        .identity
        .login("nobody@example.com", PASSWORD)
        .await;
    assert!(matches!(wrong, Err(AppError::Unauthorized)));
    assert!(matches!(unknown, Err(AppError::Unauthorized)));
    assert_eq!(count(&test, "sessions").await, 0);
}

#[tokio::test]
async fn unknown_token_is_unauthorized() {
    let (test, _) = with_household().await;
    assert!(matches!(
        test.app.identity.authenticate("nope").await,
        Err(AppError::Unauthorized)
    ));
}

#[tokio::test]
async fn expired_session_is_unauthorized_and_removed() {
    let (test, _) = with_household().await;
    let login = test
        .app
        .identity
        .login("owner@example.com", PASSWORD)
        .await
        .unwrap();
    test.clock.advance(TimeDelta::days(30));
    let result = test.app.identity.authenticate(&login.token).await;
    assert!(matches!(result, Err(AppError::Unauthorized)));
    assert_eq!(count(&test, "sessions").await, 0);
}

#[tokio::test]
async fn logout_ends_the_session() {
    let (test, _) = with_household().await;
    let login = test
        .app
        .identity
        .login("owner@example.com", PASSWORD)
        .await
        .unwrap();
    let auth = test.app.identity.authenticate(&login.token).await.unwrap();
    test.app
        .identity
        .logout(&auth.ctx, &auth.session_id)
        .await
        .unwrap();
    assert!(test.app.identity.authenticate(&login.token).await.is_err());
}

#[tokio::test]
async fn owner_adds_a_member_who_can_log_in() {
    let (test, setup) = with_household().await;
    let member = add_member(&test, &setup.ctx, "partner@example.com").await;

    assert_eq!(member.household_id, setup.ctx.household_id);
    assert_eq!(member.role, Role::Member);
    let members = test.app.identity.list_members(&setup.ctx).await.unwrap();
    assert_eq!(members.len(), 2);
    let event = last_event(&test, &setup.ctx).await;
    assert_eq!(event.event_type, "household_member.created");
    assert_eq!(event.payload["data"]["email"], "partner@example.com");
    assert_eq!(event.payload["data"]["role"], "member");
}

#[tokio::test]
async fn members_cannot_manage_members_or_settings() {
    let (test, setup) = with_household().await;
    let member = add_member(&test, &setup.ctx, "partner@example.com").await;
    let input = NewMember {
        email: "third@example.com".to_string(),
        display_name: "Third".to_string(),
        password: PASSWORD.to_string(),
    };
    let add = test
        .app
        .identity
        .add_member(&member, Origin::Api, input)
        .await;
    let remove = test
        .app
        .identity
        .remove_member(&member, Origin::Api, setup.ctx.user_id)
        .await;
    let update = test
        .app
        .identity
        .update_household(&member, Origin::Api, HouseholdUpdate::default())
        .await;
    assert!(matches!(add, Err(AppError::Forbidden)));
    assert!(matches!(remove, Err(AppError::Forbidden)));
    assert!(matches!(update, Err(AppError::Forbidden)));
}

#[tokio::test]
async fn duplicate_email_is_a_conflict() {
    let (test, setup) = with_household().await;
    let input = NewMember {
        email: "OWNER@example.com".to_string(),
        display_name: "Copy".to_string(),
        password: PASSWORD.to_string(),
    };
    let result = test
        .app
        .identity
        .add_member(&setup.ctx, Origin::Api, input)
        .await;
    assert!(matches!(result, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn removed_members_lose_their_sessions_and_tokens() {
    let (test, setup) = with_household().await;
    let member = add_member(&test, &setup.ctx, "partner@example.com").await;
    let token = test
        .app
        .identity
        .create_token(&member, "script")
        .await
        .unwrap()
        .token;
    assert!(test.app.identity.authenticate(&token).await.is_ok());

    test.app
        .identity
        .remove_member(&setup.ctx, Origin::Api, member.user_id)
        .await
        .unwrap();

    assert!(matches!(
        test.app.identity.authenticate(&token).await,
        Err(AppError::Unauthorized)
    ));
    assert!(
        test.app
            .identity
            .login("partner@example.com", PASSWORD)
            .await
            .is_err()
    );
    let event = last_event(&test, &setup.ctx).await;
    assert_eq!(event.event_type, "household_member.deleted");
    assert_eq!(event.entity_id, member.user_id.to_string());
}

#[tokio::test]
async fn a_token_of_a_removed_member_fails_even_if_the_row_survives() {
    let (test, setup) = with_household().await;
    let member = add_member(&test, &setup.ctx, "partner@example.com").await;
    let token = test
        .app
        .identity
        .create_token(&member, "script")
        .await
        .unwrap()
        .token;
    // Remove only the membership, leaving the session row in place.
    let (household, user) = (
        setup.ctx.household_id.to_string(),
        member.user_id.to_string(),
    );
    sqlx::query("DELETE FROM household_members WHERE household_id = ? AND user_id = ?")
        .bind(household)
        .bind(user)
        .execute(&test.app.pool)
        .await
        .unwrap();
    assert!(matches!(
        test.app.identity.authenticate(&token).await,
        Err(AppError::Unauthorized)
    ));
}

#[tokio::test]
async fn owner_cannot_remove_themselves_or_unknown_users() {
    let (test, setup) = with_household().await;
    let me = test
        .app
        .identity
        .remove_member(&setup.ctx, Origin::Api, setup.ctx.user_id)
        .await;
    assert!(matches!(me, Err(AppError::Conflict(_))));
    let stranger = saldo_core::UserId(uuid::Uuid::from_u128(42));
    let unknown = test
        .app
        .identity
        .remove_member(&setup.ctx, Origin::Api, stranger)
        .await;
    assert!(matches!(unknown, Err(AppError::NotFound)));
}

#[tokio::test]
async fn tokens_are_listed_authenticate_and_can_be_revoked() {
    let (test, setup) = with_household().await;
    let created = test
        .app
        .identity
        .create_token(&setup.ctx, " importer ")
        .await
        .unwrap();
    assert_eq!(created.view.name.as_deref(), Some("importer"));
    assert_eq!(created.view.expires_at, None);

    let auth = test
        .app
        .identity
        .authenticate(&created.token)
        .await
        .unwrap();
    assert!(auth.is_token);
    let listed = test.app.identity.list_tokens(&setup.ctx).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].last_used_at.is_some());

    test.app
        .identity
        .revoke_token(&setup.ctx, &created.view.id)
        .await
        .unwrap();
    assert!(
        test.app
            .identity
            .authenticate(&created.token)
            .await
            .is_err()
    );
    let again = test
        .app
        .identity
        .revoke_token(&setup.ctx, &created.view.id)
        .await;
    assert!(matches!(again, Err(AppError::NotFound)));
    assert!(
        test.app
            .identity
            .create_token(&setup.ctx, "  ")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn members_cannot_revoke_each_others_tokens() {
    let (test, setup) = with_household().await;
    let member = add_member(&test, &setup.ctx, "partner@example.com").await;
    let created = test
        .app
        .identity
        .create_token(&setup.ctx, "mine")
        .await
        .unwrap();
    let result = test
        .app
        .identity
        .revoke_token(&member, &created.view.id)
        .await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn owner_updates_household_settings() {
    let (test, setup) = with_household().await;
    let update = HouseholdUpdate {
        name: Some("Our place".to_string()),
        timezone: Some("Europe/Rome".to_string()),
        primary_currency: Some("CHF".to_string()),
    };
    let view = test
        .app
        .identity
        .update_household(&setup.ctx, Origin::Api, update)
        .await
        .unwrap();
    assert_eq!(
        (view.name.as_str(), view.timezone.as_str()),
        ("Our place", "Europe/Rome")
    );
    assert_eq!(view.primary_currency, "CHF");
    assert_eq!(
        test.app.identity.get_household(&setup.ctx).await.unwrap(),
        view
    );
    assert_eq!(
        last_event(&test, &setup.ctx).await.event_type,
        "household.updated"
    );

    let bad = HouseholdUpdate {
        primary_currency: Some("NOPE".to_string()),
        ..Default::default()
    };
    assert!(
        test.app
            .identity
            .update_household(&setup.ctx, Origin::Api, bad)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn me_describes_the_caller() {
    let (test, setup) = with_household().await;
    let me = test.app.identity.me(&setup.ctx).await.unwrap();
    assert_eq!(me.user, setup.user);
    assert_eq!(me.household.id, setup.household.id);
    assert_eq!(me.role, Role::Owner);
}

#[tokio::test]
async fn events_are_filtered_by_type_and_seq() {
    let (test, setup) = with_household().await;
    let filter = EventFilter {
        after_seq: Some(1),
        event_type: Some("account.created".to_string()),
    };
    let page = test
        .app
        .events
        .list(&setup.ctx, &filter, PageRequest::default())
        .await
        .unwrap();
    assert_eq!(page.total, 2);
    assert!(
        page.items
            .iter()
            .all(|e| e.event_type == "account.created" && e.seq > 1)
    );
}

#[tokio::test]
async fn today_follows_the_household_timezone() {
    let (test, setup) = with_household().await;
    // 2026-11-13 23:30 UTC is already the 14th in Rome and still the 13th in New York.
    test.clock.advance(TimeDelta::minutes(14 * 60 + 30));
    assert_eq!(
        test.app
            .identity
            .household_today(&setup.ctx)
            .await
            .unwrap()
            .to_string(),
        "2026-11-13"
    );
    for (zone, expected) in [
        ("Europe/Rome", "2026-11-14"),
        ("America/New_York", "2026-11-13"),
    ] {
        let update = HouseholdUpdate {
            timezone: Some(zone.into()),
            ..Default::default()
        };
        test.app
            .identity
            .update_household(&setup.ctx, Origin::Api, update)
            .await
            .unwrap();
        assert_eq!(
            test.app
                .identity
                .household_today(&setup.ctx)
                .await
                .unwrap()
                .to_string(),
            expected
        );
    }
}

#[tokio::test]
async fn unknown_timezones_are_rejected() {
    let (test, setup) = with_household().await;
    let update = HouseholdUpdate {
        timezone: Some("Mars/Olympus".into()),
        ..Default::default()
    };
    let result = test
        .app
        .identity
        .update_household(&setup.ctx, Origin::Api, update)
        .await;
    assert!(matches!(result, Err(AppError::Invalid { ref field, .. }) if field == "timezone"));
}

#[tokio::test]
async fn background_tasks_get_one_context_per_household() {
    let (test, setup) = with_household().await;
    add_member(&test, &setup.ctx, "partner@example.com").await;
    let other = other_household(&test).await;
    let mut contexts = test.app.identity.household_contexts().await.unwrap();
    contexts.sort_by_key(|c| c.household_id);
    let mut expected = vec![setup.ctx, other];
    expected.sort_by_key(|c| c.household_id);
    assert_eq!(
        contexts, expected,
        "each household once, acting as its first owner"
    );
}
