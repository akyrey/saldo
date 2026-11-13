#![allow(clippy::unwrap_used)] // test helpers

use std::time::Duration;

use saldo_importer::firefly::models::Account;
use saldo_importer::firefly::{FireflyClient, FireflyError};
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "test-token-123";

fn fixture(name: &str) -> Value {
    let text = std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn client(server: &MockServer) -> FireflyClient {
    FireflyClient::new(&server.uri(), TOKEN.to_string())
        .unwrap()
        .with_backoff(Duration::from_millis(1))
}

#[tokio::test]
async fn follows_pages_drops_duplicates_and_sends_the_token() {
    let server = MockServer::start().await;
    for page in ["1", "2"] {
        Mock::given(method("GET"))
            .and(path("/api/v1/transactions"))
            .and(query_param("page", page))
            .and(query_param("type", "all"))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(fixture(&format!("transactions-{page}.json"))),
            )
            .expect(1)
            .mount(&server)
            .await;
    }
    let groups = client(&server)
        .list::<saldo_importer::firefly::models::TransactionGroup>(
            "/v1/transactions",
            &[("type", "all")],
        )
        .await
        .unwrap();
    let ids: Vec<_> = groups.iter().map(|g| g.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"
        ]
    );
    let splits: usize = groups.iter().map(|g| g.attributes.transactions.len()).sum();
    assert_eq!(splits, 13);
}

#[tokio::test]
async fn reads_accounts_and_about() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/accounts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("accounts.json")))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/about"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("about.json")))
        .mount(&server)
        .await;
    let client = client(&server);
    let accounts = client.list::<Account>("/v1/accounts", &[]).await.unwrap();
    assert_eq!(accounts.len(), 12);
    assert_eq!(
        accounts[3].attributes.liability_direction.as_deref(),
        Some("debit")
    );
    assert_eq!(client.about().await.unwrap().version, "6.7.9");
}

#[tokio::test]
async fn retries_server_errors_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/about"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/about"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("about.json")))
        .mount(&server)
        .await;
    assert_eq!(client(&server).about().await.unwrap().api_version, "6.7.9");
}

#[tokio::test]
async fn gives_up_on_client_errors_and_persistent_failures() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/about"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/accounts"))
        .respond_with(ResponseTemplate::new(429))
        .expect(5)
        .mount(&server)
        .await;
    let client = client(&server);
    let unauthorized = client.about().await.unwrap_err();
    assert!(matches!(
        unauthorized,
        FireflyError::Status { status: 401, .. }
    ));
    let limited = client
        .list::<Account>("/v1/accounts", &[])
        .await
        .unwrap_err();
    assert!(matches!(limited, FireflyError::Status { status: 429, .. }));
    assert!(
        !format!("{unauthorized} {limited} {client:?}").contains(TOKEN),
        "the token must never be printed"
    );
}

#[tokio::test]
async fn rejects_bad_json_and_bad_urls() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/about"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "nope": 1 })))
        .mount(&server)
        .await;
    assert!(matches!(
        client(&server).about().await,
        Err(FireflyError::Decode { .. })
    ));
    assert!(matches!(
        FireflyClient::new("firefly.example", String::new()),
        Err(FireflyError::InvalidUrl(_))
    ));
    assert!(saldo_importer::firefly::check_complete("/v1/x", 3, 2).is_err());
    assert!(saldo_importer::firefly::check_complete("/v1/x", 3, 3).is_ok());
}

#[tokio::test]
async fn downloads_bytes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/attachments/1/download"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"hello".to_vec()))
        .mount(&server)
        .await;
    assert_eq!(
        client(&server)
            .get_bytes("/v1/attachments/1/download")
            .await
            .unwrap(),
        b"hello"
    );
}
