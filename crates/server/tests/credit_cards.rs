#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use axum::http::{Method, StatusCode};
use common::*;
use serde_json::{Value, json};

struct Setup {
    server: TestServer,
    auth: Auth,
    card: String,
    checking: String,
    shop: String,
}

async fn id_of(server: &TestServer, auth: &Auth, body: Value) -> String {
    let created = server.post("/api/v1/accounts", body, auth).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    created.body["data"]["id"].as_str().unwrap().to_string()
}

async fn setup() -> Setup {
    let server = server().await;
    let auth = Auth::Bearer(server.setup_and_login().await);
    let card = json!({ "name": "Visa", "type": "asset", "asset_role": "credit_card" });
    let card = id_of(&server, &auth, card).await;
    let checking = id_of(
        &server,
        &auth,
        json!({ "name": "Checking", "type": "asset" }),
    )
    .await;
    let shop = id_of(&server, &auth, json!({ "name": "Shop", "type": "expense" })).await;
    Setup {
        server,
        auth,
        card,
        checking,
        shop,
    }
}

impl Setup {
    fn path(&self, rest: &str) -> String {
        format!("/api/v1/accounts/{}/credit-card{rest}", self.card)
    }

    async fn put(&self, path: &str, body: Option<Value>) -> Response {
        self.server.send(Method::PUT, path, body, &self.auth).await
    }

    async fn buy(&self, date: &str, amount: &str) -> String {
        let body = json!({ "splits": [{ "date": date, "description": "Shopping", "source_id": self.card,
                                        "destination_id": self.shop, "amount": amount }] });
        let created = self
            .server
            .post("/api/v1/transactions", body, &self.auth)
            .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        created.body["data"]["splits"][0]["id"]
            .as_str()
            .unwrap()
            .to_string()
    }
}

#[tokio::test]
async fn manages_a_plan_its_months_and_moves_over_the_api() {
    let s = setup().await;
    let plan = json!({ "monthly_limit": "500.00", "settlement_account_id": s.checking, "settlement_day": 15 });
    let created = s.put(&s.path(""), Some(plan.clone())).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(created.body["data"]["starts_cycle"], "2026-11");
    let replaced = s.put(&s.path(""), Some(plan)).await;
    assert_eq!(replaced.status, StatusCode::OK);
    let fetched = s.server.get(&s.path(""), &s.auth).await;
    assert_eq!(fetched.body["data"]["limits"][0]["amount"], "500.00");

    let limit = s
        .put(
            &s.path("/limits/2026-12"),
            Some(json!({ "amount": "600.00" })),
        )
        .await;
    assert_eq!(limit.status, StatusCode::OK, "{}", limit.body);
    assert_eq!(limit.body["data"]["limits"][1]["from_cycle"], "2026-12");

    s.buy("2026-11-03", "100.00").await;
    let late = s.buy("2026-11-30", "50.00").await;
    let moved = s.put(&s.path(&format!("/moves/{late}")), None).await;
    assert_eq!(moved.status, StatusCode::NO_CONTENT, "{}", moved.body);

    let cycles = s.server.get(&s.path("/cycles?to=2026-12"), &s.auth).await;
    assert_eq!(cycles.status, StatusCode::OK, "{}", cycles.body);
    let months = cycles.body["data"].as_array().unwrap();
    assert_eq!(months.len(), 2);
    assert_eq!(months[0]["cycle"], "2026-12");
    assert_eq!(months[0]["limit"], "600.00");
    assert_eq!(months[0]["spent"], "50.00");
    assert_eq!(months[1]["available"], "400.00");
    assert_eq!(months[1]["settlement"]["date"], "2026-12-15");

    s.server
        .app
        .credit_cards
        .sync_settlements(&first_ctx(&s).await)
        .await
        .unwrap();
    let detail = s.server.get(&s.path("/cycles/2026-11"), &s.auth).await;
    assert_eq!(detail.status, StatusCode::OK);
    assert_eq!(detail.body["data"]["settlement"]["amount"], "100.00");
    assert_eq!(detail.body["data"]["splits"][1]["moved_out"], true);
    assert_eq!(detail.body["data"]["earlier"], json!([]));

    let summaries = s.server.get("/api/v1/credit-cards", &s.auth).await;
    assert_eq!(summaries.body["data"][0]["account_name"], "Visa");
    assert_eq!(summaries.body["data"][0]["current"]["available"], "400.00");

    let back = s
        .server
        .send(
            Method::DELETE,
            &s.path(&format!("/moves/{late}")),
            None,
            &s.auth,
        )
        .await;
    assert_eq!(back.status, StatusCode::NO_CONTENT);
    let removed = s
        .server
        .send(Method::DELETE, &s.path(""), None, &s.auth)
        .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    assert_eq!(
        s.server.get(&s.path(""), &s.auth).await.status,
        StatusCode::NOT_FOUND
    );
}

async fn first_ctx(s: &Setup) -> saldo_app::ctx::HouseholdCtx {
    s.server
        .app
        .identity
        .household_contexts()
        .await
        .unwrap()
        .remove(0)
}

#[tokio::test]
async fn rejects_bad_input_and_closed_months() {
    let s = setup().await;
    let bad_day = json!({ "monthly_limit": "1.00", "settlement_account_id": s.checking, "settlement_day": 40 });
    assert_eq!(
        s.put(&s.path(""), Some(bad_day)).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let plan = json!({ "monthly_limit": "1.00", "settlement_account_id": s.checking, "settlement_day": 1 });
    assert_eq!(
        s.put(&s.path(""), Some(plan)).await.status,
        StatusCode::CREATED
    );

    for path in [
        "/cycles?from=2026-13",
        "/cycles?from=2020-01&to=2026-11",
        "/cycles/nope",
    ] {
        let got = s.server.get(&s.path(path), &s.auth).await;
        assert_eq!(
            got.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{path}: {}",
            got.body
        );
    }
    let past = s
        .put(
            &s.path("/limits/2026-10"),
            Some(json!({ "amount": "1.00" })),
        )
        .await;
    assert_eq!(past.status, StatusCode::UNPROCESSABLE_ENTITY);

    let september = s.buy("2026-09-20", "5.00").await;
    let closed = s.put(&s.path(&format!("/moves/{september}")), None).await;
    assert_eq!(closed.status, StatusCode::CONFLICT, "{}", closed.body);
    let unknown = s
        .put(&s.path(&format!("/moves/{}", uuid::Uuid::new_v4())), None)
        .await;
    assert_eq!(unknown.status, StatusCode::UNPROCESSABLE_ENTITY);

    let other = format!("/api/v1/accounts/{}/credit-card", uuid::Uuid::new_v4());
    assert_eq!(
        s.server.get(&other, &s.auth).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        s.server.get(&s.path(""), &Auth::None).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        s.server
            .get("/api/v1/credit-cards", &Auth::None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}
