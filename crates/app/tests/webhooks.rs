#![allow(clippy::unwrap_used)] // helpers outside #[test] functions

//! Webhook endpoints (owner-only CRUD with events) and the dispatcher.

mod common;

use common::*;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::page::PageRequest;
use saldo_app::tags::NewTag;
use saldo_app::webhooks::{NewWebhook, WebhookUpdate, WebhookWithSecret};

async fn hook(
    test: &TestApp,
    ctx: &HouseholdCtx,
    types: &[&str],
    imports: bool,
) -> WebhookWithSecret {
    let input = NewWebhook {
        name: "Catcher".into(),
        url: "http://127.0.0.1:9/hook".into(),
        event_types: Some(types.iter().map(|t| t.to_string()).collect()),
        include_import_events: imports,
    };
    test.app
        .webhooks
        .create(ctx, Origin::Api, input)
        .await
        .unwrap()
}

async fn tag(test: &TestApp, ctx: &HouseholdCtx, name: &str, origin: Origin) {
    let input = NewTag {
        name: name.into(),
        description: None,
    };
    test.app.tags.create(ctx, origin, input).await.unwrap();
}

/// (event type, endpoint id) of every delivery, in event order.
async fn deliveries(test: &TestApp) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT e.event_type, d.endpoint_id FROM webhook_deliveries d
         JOIN events e ON e.seq = d.event_seq ORDER BY d.event_seq, d.endpoint_id",
    )
    .fetch_all(&test.app.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn create_shows_the_secret_once_and_never_in_events() {
    let (test, setup) = with_household().await;
    let created = hook(&test, &setup.ctx, &["*"], false).await;
    assert!(created.secret.starts_with("whsec_"));
    assert!(created.webhook.active);
    assert_eq!(created.webhook.event_types, ["*"]);

    let event = last_event(&test, &setup.ctx).await;
    assert_eq!(event.event_type, "webhook.created");
    assert_eq!(event.payload["data"]["id"], created.webhook.id.as_str());
    assert!(!event.payload.to_string().contains(&created.secret));

    let listed = test.app.webhooks.list(&setup.ctx).await.unwrap();
    assert_eq!(listed, std::slice::from_ref(&created.webhook));

    let rotated = test
        .app
        .webhooks
        .rotate_secret(&setup.ctx, Origin::Api, &created.webhook.id)
        .await
        .unwrap();
    assert_ne!(rotated.secret, created.secret);
    let event = last_event(&test, &setup.ctx).await;
    assert_eq!(event.event_type, "webhook.updated");
    assert!(!event.payload.to_string().contains(&rotated.secret));
}

#[tokio::test]
async fn defaults_to_every_event_and_validates_input() {
    let (test, setup) = with_household().await;
    let all = NewWebhook {
        name: "All".into(),
        url: "https://example.com/hook".into(),
        ..NewWebhook::default()
    };
    let created = test
        .app
        .webhooks
        .create(&setup.ctx, Origin::Api, all)
        .await
        .unwrap();
    assert_eq!(created.webhook.event_types, ["*"]);

    for (name, url, types, field) in [
        ("", "https://example.com", None, "name"),
        ("x", "ftp://example.com", None, "url"),
        ("x", "https://example.com", Some(vec![]), "event_types"),
        (
            "x",
            "https://example.com",
            Some(vec!["Bad".to_string()]),
            "event_types",
        ),
    ] {
        let input = NewWebhook {
            name: name.into(),
            url: url.into(),
            event_types: types,
            include_import_events: false,
        };
        let error = test
            .app
            .webhooks
            .create(&setup.ctx, Origin::Api, input)
            .await
            .unwrap_err();
        assert!(
            matches!(error, AppError::Invalid { field: ref f, .. } if f == field),
            "{error}"
        );
    }
}

#[tokio::test]
async fn only_owners_manage_webhooks() {
    let (test, setup) = with_household().await;
    let created = hook(&test, &setup.ctx, &["*"], false).await;
    let member = add_member(&test, &setup.ctx, "member@example.com").await;
    let id = created.webhook.id.as_str();
    let page = PageRequest::new(None, None).unwrap();
    let hooks = &test.app.webhooks;
    assert!(matches!(
        hooks.list(&member).await,
        Err(AppError::Forbidden)
    ));
    assert!(matches!(
        hooks.get(&member, id).await,
        Err(AppError::Forbidden)
    ));
    assert!(matches!(
        hooks
            .create(&member, Origin::Api, NewWebhook::default())
            .await,
        Err(AppError::Forbidden)
    ));
    assert!(matches!(
        hooks
            .update(&member, Origin::Api, id, WebhookUpdate::default())
            .await,
        Err(AppError::Forbidden)
    ));
    assert!(matches!(
        hooks.delete(&member, Origin::Api, id).await,
        Err(AppError::Forbidden)
    ));
    assert!(matches!(
        hooks.rotate_secret(&member, Origin::Api, id).await,
        Err(AppError::Forbidden)
    ));
    assert!(matches!(
        hooks.test(&member, Origin::Api, id).await,
        Err(AppError::Forbidden)
    ));
    assert!(matches!(
        hooks.deliveries(&member, id, page).await,
        Err(AppError::Forbidden)
    ));
    assert!(matches!(
        hooks.redeliver(&member, "x").await,
        Err(AppError::Forbidden)
    ));
}

#[tokio::test]
async fn other_households_cannot_see_an_endpoint() {
    let (test, setup) = with_household().await;
    let created = hook(&test, &setup.ctx, &["*"], false).await;
    let other = other_household(&test).await;
    let id = created.webhook.id.as_str();
    assert!(matches!(
        test.app.webhooks.get(&other, id).await,
        Err(AppError::NotFound)
    ));
    assert!(test.app.webhooks.list(&other).await.unwrap().is_empty());
    assert!(matches!(
        test.app.webhooks.delete(&other, Origin::Api, id).await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn update_changes_fields_and_reactivation_clears_failures() {
    let (test, setup) = with_household().await;
    let created = hook(&test, &setup.ctx, &["*"], false).await;
    let id = created.webhook.id.as_str();
    sqlx::query(
        "UPDATE webhook_endpoints SET active = 0, consecutive_failures = 20, disabled_reason = 'x'",
    )
    .execute(&test.app.pool)
    .await
    .unwrap();
    let update = WebhookUpdate {
        name: Some("Renamed".into()),
        event_types: Some(vec!["bill.*".into()]),
        include_import_events: Some(true),
        active: Some(true),
        ..WebhookUpdate::default()
    };
    let updated = test
        .app
        .webhooks
        .update(&setup.ctx, Origin::Api, id, update)
        .await
        .unwrap();
    assert_eq!(updated.name, "Renamed");
    assert_eq!(updated.event_types, ["bill.*"]);
    assert!(updated.include_import_events && updated.active);
    assert_eq!(
        (updated.consecutive_failures, updated.disabled_reason),
        (0, None)
    );
    assert_eq!(
        last_event(&test, &setup.ctx).await.event_type,
        "webhook.updated"
    );
}

#[tokio::test]
async fn delete_removes_the_endpoint_and_its_deliveries() {
    let (test, setup) = with_household().await;
    let created = hook(&test, &setup.ctx, &["*"], false).await;
    tag(&test, &setup.ctx, "a", Origin::Api).await;
    test.app.webhooks.dispatch().await.unwrap();
    assert!(count(&test, "webhook_deliveries").await > 0);
    test.app
        .webhooks
        .delete(&setup.ctx, Origin::Api, &created.webhook.id)
        .await
        .unwrap();
    assert_eq!(count(&test, "webhook_deliveries").await, 0);
    let event = last_event(&test, &setup.ctx).await;
    assert_eq!(event.event_type, "webhook.deleted");
    assert_eq!(event.payload["data"]["name"], "Catcher");
}

#[tokio::test]
async fn dispatcher_matches_types_origins_and_households() {
    let (test, setup) = with_household().await;
    // Endpoints never receive events from before they existed.
    test.clock.advance(chrono::TimeDelta::seconds(1));
    let tags_only = hook(&test, &setup.ctx, &["tag.created"], false)
        .await
        .webhook
        .id;
    test.clock.advance(chrono::TimeDelta::seconds(1));
    let everything = hook(&test, &setup.ctx, &["*"], true).await.webhook.id;
    let other = other_household(&test).await;
    tag(&test, &setup.ctx, "api", Origin::Api).await;
    tag(&test, &setup.ctx, "imported", Origin::Import).await;
    tag(&test, &other, "elsewhere", Origin::Api).await;

    let report = test.app.webhooks.dispatch().await.unwrap();
    assert_eq!(report.deliveries, 4);
    let mut expected = vec![
        // The second endpoint's own `webhook.created` is newer than the first.
        ("webhook.created".to_string(), everything.clone()),
        ("tag.created".to_string(), tags_only.clone()),
        ("tag.created".to_string(), everything.clone()),
        ("tag.created".to_string(), everything.clone()),
    ];
    let mut actual = deliveries(&test).await;
    expected.sort();
    actual.sort();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn test_events_reach_only_their_endpoint() {
    let (test, setup) = with_household().await;
    let first = hook(&test, &setup.ctx, &["bill.*"], false).await.webhook.id;
    let _second = hook(&test, &setup.ctx, &["bill.*"], false).await.webhook.id;
    test.app
        .webhooks
        .test(&setup.ctx, Origin::Api, &first)
        .await
        .unwrap();
    test.app.webhooks.dispatch().await.unwrap();
    assert_eq!(
        deliveries(&test).await,
        [("webhook.test".to_string(), first)]
    );
}

#[tokio::test]
async fn a_crash_between_batches_loses_and_duplicates_nothing() {
    let (test, setup) = with_household().await;
    hook(&test, &setup.ctx, &["tag.*"], false).await;
    for name in ["a", "b", "c", "d", "e"] {
        tag(&test, &setup.ctx, name, Origin::Api).await;
    }
    // Two batches, then the process "dies": a new service on the same
    // database picks up from the cursor.
    test.app.webhooks.dispatch_batch(2).await.unwrap();
    test.app.webhooks.dispatch_batch(2).await.unwrap();
    let restarted = saldo_app::App::new(test.app.pool.clone(), test.clock.clone());
    restarted.webhooks.dispatch().await.unwrap();
    restarted.webhooks.dispatch().await.unwrap();
    assert_eq!(deliveries(&test).await.len(), 5);
    let (cursor,): (i64,) =
        sqlx::query_as("SELECT last_seq FROM consumer_cursors WHERE consumer = 'webhooks'")
            .fetch_one(&test.app.pool)
            .await
            .unwrap();
    let (max,): (i64,) = sqlx::query_as("SELECT max(seq) FROM events")
        .fetch_one(&test.app.pool)
        .await
        .unwrap();
    assert_eq!(cursor, max);
}

#[tokio::test]
async fn events_from_before_an_endpoint_and_inactive_endpoints_get_nothing() {
    let (test, setup) = with_household().await;
    tag(&test, &setup.ctx, "old", Origin::Api).await;
    test.clock.advance(chrono::TimeDelta::seconds(1));
    let id = hook(&test, &setup.ctx, &["tag.*"], false).await.webhook.id;
    test.app.webhooks.dispatch().await.unwrap();
    assert!(deliveries(&test).await.is_empty());
    let off = WebhookUpdate {
        active: Some(false),
        ..WebhookUpdate::default()
    };
    test.app
        .webhooks
        .update(&setup.ctx, Origin::Api, &id, off)
        .await
        .unwrap();
    tag(&test, &setup.ctx, "new", Origin::Api).await;
    test.app.webhooks.dispatch().await.unwrap();
    assert!(deliveries(&test).await.is_empty());
}
