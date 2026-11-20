//! Which channels a notifying event reaches (Phase 10): only those that
//! chose its type, and recurrences only when the scheduler booked them.

#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use common::*;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::notifications::{ChannelUpdate, ChannelView, NewChannel};
use saldo_app::recurrences::{RecurrenceInput, RepetitionInput, TemplateSplitInput};
use serde_json::json;

async fn channel(test: &TestApp, ctx: &HouseholdCtx, types: Option<Vec<&str>>) -> ChannelView {
    let input = NewChannel {
        kind: "ntfy".into(),
        name: "Phone".into(),
        config: json!({ "server_url": "https://ntfy.example.com", "topic": "saldo" }),
        active: None,
        event_types: types.map(|t| t.into_iter().map(str::to_string).collect()),
    };
    test.app
        .notifications
        .create_channel(ctx, Origin::Api, input)
        .await
        .unwrap()
}

/// `(channel id, dedupe key)` of every queued notification.
async fn queued(test: &TestApp) -> Vec<(String, String)> {
    while test.app.notifications.fan_out(100).await.unwrap() == 100 {}
    sqlx::query_as("SELECT channel_id, dedupe_key FROM notifications ORDER BY dedupe_key")
        .fetch_all(&test.app.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn channels_default_to_bills_and_validate_their_types() {
    let (test, setup) = with_household().await;
    let plain = channel(&test, &setup.ctx, None).await;
    assert_eq!(plain.event_types, ["bill.due_soon", "bill.overdue"]);

    let update = ChannelUpdate {
        event_types: Some(vec!["recurrence.fired".into(), "recurrence.fired".into()]),
        ..ChannelUpdate::default()
    };
    let updated = test
        .app
        .notifications
        .update_channel(&setup.ctx, Origin::Api, plain.id.parse().unwrap(), update)
        .await
        .unwrap();
    assert_eq!(updated.event_types, ["recurrence.fired"], "repeats dropped");

    let bad = ChannelUpdate {
        event_types: Some(vec!["transaction.created".into()]),
        ..ChannelUpdate::default()
    };
    let result = test
        .app
        .notifications
        .update_channel(&setup.ctx, Origin::Api, plain.id.parse().unwrap(), bad)
        .await;
    assert!(
        matches!(result, Err(AppError::Invalid { ref field, .. }) if field == "event_types"),
        "{result:?}"
    );
}

#[tokio::test]
async fn scheduled_recurrences_reach_only_subscribed_channels() {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let bills_only = channel(&test, &ctx, None).await;
    let recurring = channel(&test, &ctx, Some(vec!["recurrence.fired"])).await;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let landlord = account(&test, &ctx, "Landlord", "expense", None).await;
    let input = RecurrenceInput {
        title: "Monthly rent".into(),
        description: None,
        first_date: date(1),
        repeat_until: None,
        max_occurrences: None,
        apply_rules: None,
        active: None,
        notes: None,
        repetitions: vec![RepetitionInput {
            kind: "monthly".into(),
            moment: "1".into(),
            skip: 0,
            weekend_policy: None,
        }],
        splits: vec![TemplateSplitInput {
            description: "Rent".into(),
            source_id: account_id(&checking),
            destination_id: account_id(&landlord),
            amount: "800".into(),
            currency: None,
            foreign_amount: None,
            foreign_currency: None,
            category_id: None,
            budget_id: None,
            bill_id: None,
            piggy_bank_id: None,
            tag_ids: Vec::new(),
        }],
    };
    let recurrence = test
        .app
        .recurrences
        .create(&ctx, Origin::Api, input)
        .await
        .unwrap();
    let id = recurrence.id.parse().unwrap();
    let service = &test.app.recurrences;

    // "Fire now" by a person: no notification.
    service.fire(&ctx, Origin::Api, id, date(5)).await.unwrap();
    assert!(queued(&test).await.is_empty());
    // Booked by the scheduler: one notification, for the subscribed channel.
    service
        .fire(&ctx, Origin::Scheduler, id, date(1))
        .await
        .unwrap();
    // Firing the same date again creates nothing and notifies nothing.
    service
        .fire(&ctx, Origin::Scheduler, id, date(1))
        .await
        .unwrap();
    let rows = queued(&test).await;
    assert_eq!(
        rows,
        [(
            recurring.id.clone(),
            format!("recurrence.fired:{}:2026-11-01", recurrence.id)
        )]
    );
    assert_ne!(rows[0].0, bills_only.id);

    let pending = test.app.notifications.pending(10).await.unwrap();
    let message = pending[0].message.clone().unwrap();
    assert_eq!(message.title, "Recurring transaction created: Monthly rent");
    assert_eq!(
        message.body,
        "800.00 EUR from Checking to Landlord on 2026-11-01."
    );
}
