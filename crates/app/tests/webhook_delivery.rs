#![allow(clippy::unwrap_used)] // helpers outside #[test] functions

//! The delivery worker against a local `wiremock` receiver, with the fake clock.

mod common;

use std::time::Duration;

use chrono::TimeDelta;
use common::*;
use saldo_app::clock::Clock;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::page::PageRequest;
use saldo_app::tags::NewTag;
use saldo_app::webhooks::signing::verify;
use saldo_app::webhooks::{
    DeliveryReport, DeliveryView, NewWebhook, WebhookService, WebhookWithSecret,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct Fixture {
    test: TestApp,
    ctx: HouseholdCtx,
    hook: WebhookWithSecret,
    service: WebhookService,
    server: MockServer,
}

/// A household with one endpoint for `tag.*` at a fresh mock server, and one
/// dispatched `tag.created` delivery.
async fn fixture() -> Fixture {
    let (test, setup) = with_household().await;
    let server = MockServer::start().await;
    test.clock.advance(TimeDelta::seconds(1));
    let input = NewWebhook {
        name: "Receiver".into(),
        url: format!("{}/hook", server.uri()),
        event_types: Some(vec!["tag.*".into()]),
        include_import_events: false,
    };
    let hook = test
        .app
        .webhooks
        .create(&setup.ctx, Origin::Api, input)
        .await
        .unwrap();
    let tag = NewTag {
        name: "groceries".into(),
        description: None,
    };
    test.app
        .tags
        .create(&setup.ctx, Origin::Api, tag)
        .await
        .unwrap();
    test.app.webhooks.dispatch().await.unwrap();
    let service = test
        .app
        .webhooks
        .clone()
        .with_timeout(Duration::from_millis(300));
    Fixture {
        ctx: setup.ctx,
        test,
        hook,
        service,
        server,
    }
}

async fn reply(server: &MockServer, status: u16, times: u64) {
    Mock::given(method("POST"))
        .and(path("/hook"))
        .respond_with(ResponseTemplate::new(status))
        .up_to_n_times(times)
        .mount(server)
        .await;
}

impl Fixture {
    async fn delivery(&self) -> DeliveryView {
        let page = PageRequest::new(None, None).unwrap();
        let mut items = self
            .service
            .deliveries(&self.ctx, &self.hook.webhook.id, page)
            .await
            .unwrap()
            .items;
        assert_eq!(items.len(), 1);
        items.remove(0)
    }

    async fn run(&self) -> DeliveryReport {
        self.service.deliver_due().await.unwrap()
    }
}

#[tokio::test]
async fn a_2xx_succeeds_with_headers_and_a_verifiable_signature() {
    let f = fixture().await;
    reply(&f.server, 204, 1).await;
    let report = f.run().await;
    assert_eq!(report.succeeded, 1);

    let requests = f.server.received_requests().await.unwrap();
    let request = requests.first().unwrap();
    let header = |name: &str| {
        request
            .headers
            .get(name)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string()
    };
    let delivery = f.delivery().await;
    assert_eq!(header("content-type"), "application/json");
    assert!(header("user-agent").starts_with("Saldo-Webhook/"));
    assert_eq!(header("x-saldo-event-type"), "tag.created");
    assert_eq!(header("x-saldo-event-id"), delivery.event_id);
    assert_eq!(header("x-saldo-delivery-id"), delivery.id);
    let now = f.test.clock.now().timestamp();
    verify(
        &f.hook.secret,
        &header("x-saldo-signature"),
        &request.body,
        now,
        300,
    )
    .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["type"], "tag.created");
    assert_eq!(body["data"]["name"], "groceries");

    assert_eq!(
        (
            delivery.status.as_str(),
            delivery.attempts,
            delivery.last_status_code
        ),
        ("succeeded", 1, Some(204))
    );
    assert_eq!(delivery.next_attempt_at, None);
    // Nothing left to send.
    assert_eq!(f.run().await, DeliveryReport::default());
}

#[tokio::test]
async fn a_500_is_retried_after_the_backoff() {
    let f = fixture().await;
    reply(&f.server, 500, 1).await;
    reply(&f.server, 200, 1).await;
    assert_eq!(f.run().await.retrying, 1);
    let failed = f.delivery().await;
    assert_eq!(
        (
            failed.status.as_str(),
            failed.attempts,
            failed.last_status_code
        ),
        ("pending", 1, Some(500))
    );
    assert_eq!(
        failed.last_error.as_deref(),
        Some("HTTP 500 Internal Server Error")
    );

    // Not due yet: one minute plus up to 10% jitter.
    f.test.clock.advance(TimeDelta::seconds(59));
    assert_eq!(f.run().await, DeliveryReport::default());
    f.test.clock.advance(TimeDelta::seconds(7));
    assert_eq!(f.run().await.succeeded, 1);
    let done = f.delivery().await;
    assert_eq!(
        (done.status.as_str(), done.attempts, done.last_error),
        ("succeeded", 2, None)
    );
}

#[tokio::test]
async fn a_timeout_counts_as_a_failure() {
    let f = fixture().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(2)))
        .mount(&f.server)
        .await;
    assert_eq!(f.run().await.retrying, 1);
    let delivery = f.delivery().await;
    assert_eq!(delivery.last_status_code, None);
    assert!(delivery.last_error.unwrap().starts_with("timed out"));
}

#[tokio::test]
async fn redirects_are_not_followed() {
    let f = fixture().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/elsewhere"))
        .mount(&f.server)
        .await;
    assert_eq!(f.run().await.retrying, 1);
    assert_eq!(f.delivery().await.last_status_code, Some(302));
    assert_eq!(f.server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn eight_failures_make_a_delivery_dead_and_redeliver_revives_it() {
    let f = fixture().await;
    reply(&f.server, 503, 100).await;
    for _ in 0..7 {
        assert_eq!(f.run().await.retrying, 1);
        f.test.clock.advance(TimeDelta::hours(27));
    }
    assert_eq!(f.run().await.dead, 1);
    let dead = f.delivery().await;
    assert_eq!(
        (dead.status.as_str(), dead.attempts, dead.next_attempt_at),
        ("dead", 8, None)
    );
    let endpoint = f.service.get(&f.ctx, &f.hook.webhook.id).await.unwrap();
    assert_eq!(endpoint.consecutive_failures, 1);
    assert!(endpoint.active);
    f.test.clock.advance(TimeDelta::days(30));
    assert_eq!(f.run().await, DeliveryReport::default());

    let revived = f.service.redeliver(&f.ctx, &dead.id).await.unwrap();
    assert_eq!((revived.status.as_str(), revived.attempts), ("pending", 0));
    f.server.reset().await;
    reply(&f.server, 200, 1).await;
    assert_eq!(f.run().await.succeeded, 1);
    let endpoint = f.service.get(&f.ctx, &f.hook.webhook.id).await.unwrap();
    assert_eq!(endpoint.consecutive_failures, 0);
}

#[tokio::test]
async fn the_twentieth_dead_delivery_deactivates_the_endpoint() {
    let f = fixture().await;
    reply(&f.server, 500, 1).await;
    sqlx::query("UPDATE webhook_endpoints SET consecutive_failures = 19")
        .execute(&f.test.app.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE webhook_deliveries SET attempts = 7")
        .execute(&f.test.app.pool)
        .await
        .unwrap();
    assert_eq!(f.run().await.dead, 1);
    let endpoint = f.service.get(&f.ctx, &f.hook.webhook.id).await.unwrap();
    assert!(!endpoint.active);
    assert_eq!(endpoint.consecutive_failures, 20);
    assert!(endpoint.disabled_reason.unwrap().contains("20 consecutive"));
    let event = last_event(&f.test, &f.ctx).await;
    assert_eq!(
        (event.event_type.as_str(), event.origin.as_str()),
        ("webhook.updated", "system")
    );
    assert_eq!(event.payload["data"]["active"], false);

    // Pending deliveries of an inactive endpoint wait.
    let revived = f
        .service
        .redeliver(&f.ctx, &f.delivery().await.id)
        .await
        .unwrap();
    assert_eq!(revived.status, "pending");
    assert_eq!(f.run().await, DeliveryReport::default());
}

#[tokio::test]
async fn an_unreachable_receiver_is_a_connection_failure() {
    let f = fixture().await;
    let url = "http://127.0.0.1:9/hook";
    sqlx::query("UPDATE webhook_endpoints SET url = ?")
        .bind(url)
        .execute(&f.test.app.pool)
        .await
        .unwrap();
    assert_eq!(f.run().await.retrying, 1);
    let error = f.delivery().await.last_error.unwrap();
    assert!(!error.contains(url), "{error}");
}
