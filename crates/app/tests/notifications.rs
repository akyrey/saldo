//! Notification channels and the notifier outbox (ARCHITECTURE.md 8.4).

#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use chrono::TimeDelta;
use common::*;
use saldo_app::bills::{NewBill, run_reminders};
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::notifications::{
    ChannelConfig, ChannelUpdate, ChannelView, MAX_ATTEMPTS, NewChannel,
};
use serde_json::json;
use uuid::Uuid;

fn ntfy(name: &str, token: Option<&str>) -> NewChannel {
    NewChannel {
        kind: "ntfy".into(),
        name: name.into(),
        config: json!({ "server_url": "https://ntfy.example.com", "topic": "bills", "token": token }),
        active: None,
        event_types: None,
    }
}

async fn create(test: &TestApp, ctx: &HouseholdCtx, input: NewChannel) -> ChannelView {
    test.app
        .notifications
        .create_channel(ctx, Origin::Api, input)
        .await
        .unwrap()
}

async fn rows(test: &TestApp) -> Vec<(String, String, String, i64)> {
    sqlx::query_as("SELECT channel_id, dedupe_key, status, attempts FROM notifications ORDER BY channel_id, dedupe_key")
        .fetch_all(&test.app.pool)
        .await
        .unwrap()
}

/// A bill due tomorrow, and one scheduler run: one `bill.due_soon` event.
async fn due_soon_event(test: &TestApp, ctx: &HouseholdCtx) {
    let bill = NewBill {
        name: "Rent".into(),
        amount_min: "800".into(),
        amount_max: "800".into(),
        currency: None,
        first_due_date: "2026-11-14".parse().unwrap(),
        end_date: None,
        repeat_freq: "monthly".into(),
        skip: None,
        remind_days_before: None,
        remind_every_days: None,
        active: None,
        sort_order: None,
        notes: None,
    };
    test.app.bills.create(ctx, Origin::Api, bill).await.unwrap();
    assert_eq!(
        run_reminders(&test.app.identity, &test.app.bills)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn channels_belong_to_their_member_and_events_hold_no_secrets() {
    let (test, setup) = with_household().await;
    let partner = add_member(&test, &setup.ctx, "partner@example.com").await;
    let mine = create(&test, &setup.ctx, ntfy("Phone", Some("tk_secret"))).await;
    assert_eq!((mine.kind.as_str(), mine.active), ("ntfy", true));
    assert!(mine.settings.has_token);

    let event = last_event(&test, &setup.ctx).await;
    assert_eq!(event.event_type, "notification_channel.created");
    assert!(!event.payload.to_string().contains("tk_secret"));

    let service = &test.app.notifications;
    assert!(service.list_channels(&partner).await.unwrap().is_empty());
    let id: Uuid = mine.id.parse().unwrap();
    for result in [
        service.channel(&partner, id).await.map(|_| ()),
        service.delete_channel(&partner, Origin::Api, id).await,
    ] {
        assert!(matches!(result, Err(AppError::NotFound)));
    }
    let channel = service.channel(&setup.ctx, id).await.unwrap();
    assert!(
        matches!(channel.config, ChannelConfig::Ntfy { token: Some(ref t), .. } if t == "tk_secret")
    );
}

#[tokio::test]
async fn updates_merge_the_config_and_delete_removes_the_channel() {
    let (test, setup) = with_household().await;
    let created = create(&test, &setup.ctx, ntfy("Phone", Some("tk"))).await;
    let id: Uuid = created.id.parse().unwrap();
    let service = &test.app.notifications;
    let update = ChannelUpdate {
        name: Some(" Laptop ".into()),
        config: Some(json!({ "topic": "renamed" })),
        active: Some(false),
        event_types: None,
    };
    let updated = service
        .update_channel(&setup.ctx, Origin::Api, id, update)
        .await
        .unwrap();
    assert_eq!(updated.name, "Laptop");
    assert_eq!(updated.settings.topic.as_deref(), Some("renamed"));
    assert!(updated.settings.has_token && !updated.active);

    let invalid = ChannelUpdate {
        config: Some(json!({ "server_url": "nope" })),
        ..Default::default()
    };
    let result = service
        .update_channel(&setup.ctx, Origin::Api, id, invalid)
        .await;
    assert!(
        matches!(result, Err(AppError::Invalid { ref field, .. }) if field == "config.server_url")
    );

    service
        .delete_channel(&setup.ctx, Origin::Api, id)
        .await
        .unwrap();
    assert!(service.list_channels(&setup.ctx).await.unwrap().is_empty());
    let types: Vec<_> = events(&test, &setup.ctx)
        .await
        .into_iter()
        .map(|e| e.event_type)
        .filter(|t| t.starts_with("notification_channel."))
        .collect();
    assert_eq!(
        types,
        [
            "notification_channel.created",
            "notification_channel.updated",
            "notification_channel.deleted"
        ]
    );
}

#[tokio::test]
async fn bad_input_is_rejected_with_the_field() {
    let (test, setup) = with_household().await;
    let service = &test.app.notifications;
    let cases: [(NewChannel, &str); 3] = [
        (
            NewChannel {
                kind: "sms".into(),
                ..ntfy("x", None)
            },
            "kind",
        ),
        (
            NewChannel {
                name: "  ".into(),
                ..ntfy("x", None)
            },
            "name",
        ),
        (
            NewChannel {
                kind: "gotify".into(),
                ..ntfy("x", None)
            },
            "config",
        ),
    ];
    for (input, field) in cases {
        let result = service.create_channel(&setup.ctx, Origin::Api, input).await;
        assert!(
            matches!(result, Err(AppError::Invalid { field: ref f, .. }) if f == field),
            "{field}"
        );
    }
}

#[tokio::test]
async fn reminders_fan_out_to_every_active_channel_of_every_member() {
    let (test, setup) = with_household().await;
    let partner = add_member(&test, &setup.ctx, "partner@example.com").await;
    let mine = create(&test, &setup.ctx, ntfy("Mine", None)).await;
    let theirs = create(&test, &partner, ntfy("Theirs", None)).await;
    let off = NewChannel {
        active: Some(false),
        ..ntfy("Off", None)
    };
    create(&test, &partner, off).await;
    // Another household's channel never hears about this household's bills.
    let other = other_household(&test).await;
    create(&test, &other, ntfy("Other", None)).await;

    due_soon_event(&test, &setup.ctx).await;
    let service = &test.app.notifications;
    let read = service.fan_out(500).await.unwrap();
    assert!(read > 0);
    assert_eq!(service.fan_out(500).await.unwrap(), 0, "caught up");

    let found = rows(&test).await;
    let mut channels: Vec<_> = found.iter().map(|r| r.0.clone()).collect();
    channels.sort();
    let mut expected = vec![mine.id.clone(), theirs.id.clone()];
    expected.sort();
    assert_eq!(channels, expected);
    assert!(
        found
            .iter()
            .all(|r| r.1.starts_with("bill.due_soon:") && r.1.ends_with(":2026-11-13"))
    );

    let pending = service.pending(10).await.unwrap();
    assert_eq!(pending.len(), 2);
    let message = pending[0].message.clone().unwrap();
    assert_eq!(message.title, "Bill due soon: Rent");
}

#[tokio::test]
async fn replaying_events_creates_no_duplicates() {
    let (test, setup) = with_household().await;
    create(&test, &setup.ctx, ntfy("Mine", None)).await;
    due_soon_event(&test, &setup.ctx).await;
    let service = &test.app.notifications;
    service.fan_out(500).await.unwrap();
    // As if the cursor update had been lost: everything is read again.
    sqlx::query("UPDATE consumer_cursors SET last_seq = 0")
        .execute(&test.app.pool)
        .await
        .unwrap();
    service.fan_out(500).await.unwrap();
    assert_eq!(rows(&test).await.len(), 1);
}

#[tokio::test]
async fn small_batches_advance_the_cursor_step_by_step() {
    let (test, setup) = with_household().await;
    create(&test, &setup.ctx, ntfy("Mine", None)).await;
    due_soon_event(&test, &setup.ctx).await;
    let service = &test.app.notifications;
    let mut total = 0;
    loop {
        let read = service.fan_out(2).await.unwrap();
        total += read;
        if read < 2 {
            break;
        }
    }
    assert_eq!(total, events(&test, &setup.ctx).await.len());
    assert_eq!(rows(&test).await.len(), 1);
}

#[tokio::test]
async fn sends_are_recorded_and_failures_give_up_after_four_attempts() {
    let (test, setup) = with_household().await;
    let partner = add_member(&test, &setup.ctx, "partner@example.com").await;
    create(&test, &setup.ctx, ntfy("Mine", None)).await;
    create(&test, &partner, ntfy("Theirs", None)).await;
    due_soon_event(&test, &setup.ctx).await;
    let service = &test.app.notifications;
    service.fan_out(500).await.unwrap();
    let pending = service.pending(10).await.unwrap();
    let (good, bad) = (&pending[0].id, &pending[1].id);

    service.mark_sent(good).await.unwrap();
    for attempt in 1..=MAX_ATTEMPTS {
        test.clock.advance(TimeDelta::minutes(1));
        let gave_up = service.mark_attempt_failed(bad, "HTTP 500").await.unwrap();
        assert_eq!(gave_up, attempt == MAX_ATTEMPTS);
    }
    assert!(service.pending(10).await.unwrap().is_empty());
    let statuses: Vec<(String, i64)> = rows(&test).await.into_iter().map(|r| (r.2, r.3)).collect();
    assert!(statuses.contains(&("sent".to_string(), 1)));
    assert!(statuses.contains(&("failed".to_string(), MAX_ATTEMPTS)));
    // Already final: nothing changes any more.
    assert!(!service.mark_attempt_failed(bad, "again").await.unwrap());
}

#[tokio::test]
async fn paused_channels_hold_their_pending_notifications() {
    let (test, setup) = with_household().await;
    let channel = create(&test, &setup.ctx, ntfy("Mine", None)).await;
    due_soon_event(&test, &setup.ctx).await;
    let service = &test.app.notifications;
    service.fan_out(500).await.unwrap();
    let pause = ChannelUpdate {
        active: Some(false),
        ..Default::default()
    };
    service
        .update_channel(&setup.ctx, Origin::Api, channel.id.parse().unwrap(), pause)
        .await
        .unwrap();
    assert!(service.pending(10).await.unwrap().is_empty());
}
