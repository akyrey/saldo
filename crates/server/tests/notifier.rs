//! The notifier task end to end with fake senders: reminders become one
//! send per channel, replays and restarts send nothing twice, and a failing
//! channel does not hold up the others.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use saldo_app::bills::{NewBill, run_reminders};
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::events::EventFilter;
use saldo_app::identity::SetupInput;
use saldo_app::notifications::{Channel, Message, NewChannel};
use saldo_app::page::PageRequest;
use saldo_server::notify::{Notifier, NotifyError, Senders};
use saldo_server::workers::notifier::run_notifier;
use serde_json::json;

mod common;

/// Records what it was asked to send; fails every time if `fail` is set.
struct Fake {
    sent: Arc<Mutex<Vec<String>>>,
    fail: bool,
}

#[async_trait]
impl Notifier for Fake {
    async fn send(&self, channel: &Channel, message: &Message) -> Result<(), NotifyError> {
        self.sent
            .lock()
            .unwrap()
            .push(format!("{}: {}", channel.name, message.title));
        if self.fail {
            Err(NotifyError::Status(500))
        } else {
            Ok(())
        }
    }
}

/// A push service that no longer knows the subscription (HTTP 410).
struct GonePush {
    sent: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl Notifier for GonePush {
    async fn send(&self, channel: &Channel, message: &Message) -> Result<(), NotifyError> {
        self.sent
            .lock()
            .unwrap()
            .push(format!("{}: {}", channel.name, message.title));
        Err(NotifyError::SubscriptionGone(410))
    }
}

/// ntfy channels succeed, Gotify channels always fail.
fn fake_senders() -> (Senders, Arc<Mutex<Vec<String>>>) {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let senders = Senders {
        ntfy: Box::new(Fake {
            sent: sent.clone(),
            fail: false,
        }),
        gotify: Box::new(Fake {
            sent: sent.clone(),
            fail: true,
        }),
        email: None,
        webpush: None,
    };
    (senders, sent)
}

async fn household(server: &common::TestServer) -> HouseholdCtx {
    let input = SetupInput {
        email: "owner@example.com".into(),
        password: common::PASSWORD.into(),
        display_name: "Owner".into(),
        household_name: "Home".into(),
        primary_currency: "EUR".into(),
    };
    server
        .app
        .identity
        .setup(Origin::Api, input)
        .await
        .unwrap()
        .ctx
}

async fn channel(server: &common::TestServer, ctx: &HouseholdCtx, kind: &str, name: &str) {
    let config = match kind {
        "ntfy" => json!({ "server_url": "https://ntfy.example.com", "topic": "bills" }),
        "webpush" => json!({
            "endpoint": "https://push.example.com/send/abc",
            "keys": {
                "p256dh": "BLn9b-VR0ca83knDNZ32dCHGyjJp-1riX9ZTN40MqV8K_LpQmLqxC_DoHvqvFXO_nGdAB4W9dogZb_sM-uV4JbY",
                "auth": "_ordMnz7uTCmrpBTeUV4Bw"
            }
        }),
        _ => json!({ "server_url": "https://gotify.example.com", "token": "tk" }),
    };
    let input = NewChannel {
        kind: kind.into(),
        name: name.into(),
        config,
        active: None,
        event_types: None,
    };
    server
        .app
        .notifications
        .create_channel(ctx, Origin::Api, input)
        .await
        .unwrap();
}

/// A bill due tomorrow and one scheduler run: one `bill.due_soon`.
async fn reminder(server: &common::TestServer, ctx: &HouseholdCtx) {
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
    server
        .app
        .bills
        .create(ctx, Origin::Api, bill)
        .await
        .unwrap();
    run_reminders(&server.app.identity, &server.app.bills)
        .await
        .unwrap();
}

async fn statuses(server: &common::TestServer) -> Vec<(String, i64)> {
    sqlx::query_as(
        "SELECT n.status, n.attempts FROM notifications n
         JOIN notification_channels c ON c.id = n.channel_id ORDER BY c.name",
    )
    .fetch_all(&server.app.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn one_failing_channel_does_not_block_the_others_and_gives_up_after_four_tries() {
    let server = common::server().await;
    let ctx = household(&server).await;
    channel(&server, &ctx, "gotify", "A broken").await;
    channel(&server, &ctx, "ntfy", "B phone").await;
    reminder(&server, &ctx).await;
    let (senders, sent) = fake_senders();

    run_notifier(&server.app, &senders).await.unwrap();
    let mut first = sent.lock().unwrap().clone();
    first.sort();
    assert_eq!(
        first,
        [
            "A broken: Bill due soon: Rent",
            "B phone: Bill due soon: Rent"
        ]
    );
    assert_eq!(
        statuses(&server).await,
        [("pending".to_string(), 1), ("sent".to_string(), 1)]
    );

    for _ in 0..5 {
        run_notifier(&server.app, &senders).await.unwrap();
    }
    assert_eq!(
        statuses(&server).await,
        [("failed".to_string(), 4), ("sent".to_string(), 1)]
    );
    // Four tries of the broken channel, one send to the phone.
    assert_eq!(sent.lock().unwrap().len(), 5);
}

#[tokio::test]
async fn replays_and_restarts_send_nothing_twice() {
    let server = common::server().await;
    let ctx = household(&server).await;
    channel(&server, &ctx, "ntfy", "Phone").await;
    reminder(&server, &ctx).await;
    let (senders, sent) = fake_senders();
    run_notifier(&server.app, &senders).await.unwrap();

    // A "restart": a fresh set of senders, the same database.
    let (restarted, sent_again) = fake_senders();
    run_notifier(&server.app, &restarted).await.unwrap();
    // A replay of the whole event log, as if the cursor had been lost.
    sqlx::query("UPDATE consumer_cursors SET last_seq = 0")
        .execute(&server.app.pool)
        .await
        .unwrap();
    run_notifier(&server.app, &restarted).await.unwrap();

    assert_eq!(sent.lock().unwrap().len(), 1);
    assert!(sent_again.lock().unwrap().is_empty());
    assert_eq!(statuses(&server).await, [("sent".to_string(), 1)]);
}

#[tokio::test]
async fn channels_without_a_configured_sender_fail_and_events_without_text_are_skipped() {
    let server = common::server().await;
    let ctx = household(&server).await;
    // Only household events, no reminder: nothing to send.
    let (senders, sent) = fake_senders();
    run_notifier(&server.app, &senders).await.unwrap();
    assert!(sent.lock().unwrap().is_empty());

    let input = NewChannel {
        kind: "email".into(),
        name: "Mail".into(),
        config: json!({ "to": "me@example.com" }),
        active: None,
        event_types: None,
    };
    server
        .app
        .notifications
        .create_channel(&ctx, Origin::Api, input)
        .await
        .unwrap();
    reminder(&server, &ctx).await;
    run_notifier(&server.app, &senders).await.unwrap();
    let error: (String,) = sqlx::query_as("SELECT last_error FROM notifications")
        .fetch_one(&server.app.pool)
        .await
        .unwrap();
    assert!(error.0.contains("not configured"), "{}", error.0);
}

#[tokio::test]
async fn a_push_subscription_that_is_gone_pauses_its_channel_at_once() {
    let server = common::server().await;
    let ctx = household(&server).await;
    channel(&server, &ctx, "webpush", "A old laptop").await;
    channel(&server, &ctx, "ntfy", "B phone").await;
    reminder(&server, &ctx).await;
    let (mut senders, sent) = fake_senders();
    senders.webpush = Some(Box::new(GonePush { sent: sent.clone() }));

    run_notifier(&server.app, &senders).await.unwrap();
    // Later runs do not try the dead subscription again.
    run_notifier(&server.app, &senders).await.unwrap();

    assert_eq!(sent.lock().unwrap().len(), 2, "one try each");
    assert_eq!(
        statuses(&server).await,
        [("failed".to_string(), 1), ("sent".to_string(), 1)]
    );
    let channels = server.app.notifications.list_channels(&ctx).await.unwrap();
    let active: Vec<(&str, bool)> = channels
        .iter()
        .map(|c| (c.name.as_str(), c.active))
        .collect();
    assert!(active.contains(&("A old laptop", false)), "{active:?}");
    assert!(active.contains(&("B phone", true)), "{active:?}");
    let filter = EventFilter {
        event_type: Some("notification_channel.updated".into()),
        ..EventFilter::default()
    };
    let page = PageRequest::new(None, Some(10)).unwrap();
    let events = server.app.events.list(&ctx, &filter, page).await.unwrap();
    assert_eq!(events.items.len(), 1);
    assert_eq!(events.items[0].origin, "system");
    assert_eq!(events.items[0].payload["data"]["active"], false);
}
