//! The channel senders against local fakes: wiremock for HTTP, a tiny SMTP
//! server for email. Nothing contacts a real service.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use saldo_app::notifications::{Channel, ChannelConfig, ChannelKind, Message};
use saldo_server::config::Config;
use saldo_server::notify::config::{SmtpConfig, SmtpTls, VapidConfig, generate_vapid_keys};
use saldo_server::notify::{
    EmailNotifier, GotifyNotifier, Notifier, NotifyError, NtfyNotifier, Senders, WebPushNotifier,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use wiremock::matchers::{body_partial_json, header, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PUBLIC_URL: &str = "https://money.example.com";
const P256DH: &str =
    "BLn9b-VR0ca83knDNZ32dCHGyjJp-1riX9ZTN40MqV8K_LpQmLqxC_DoHvqvFXO_nGdAB4W9dogZb_sM-uV4JbY";
const AUTH: &str = "_ordMnz7uTCmrpBTeUV4Bw";

fn channel(config: ChannelConfig) -> Channel {
    Channel {
        id: "c1".into(),
        household_id: "h1".into(),
        user_id: "u1".into(),
        name: "Test".into(),
        active: true,
        config,
    }
}

fn message() -> Message {
    Message {
        title: "Bill overdue: Rent".into(),
        body: "800.00 EUR was due on 2026-11-20 (1 day overdue).".into(),
        link: "/bills/b1".into(),
        tag: "bill-b1".into(),
    }
}

fn ntfy(server: &MockServer, token: Option<&str>) -> Channel {
    channel(ChannelConfig::Ntfy {
        server_url: server.uri(),
        topic: "bills".into(),
        token: token.map(str::to_string),
    })
}

#[tokio::test]
async fn ntfy_publishes_json_with_the_token_as_bearer() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .and(header("authorization", "Bearer tk_123"))
        .and(body_partial_json(serde_json::json!({
            "topic": "bills",
            "title": "Bill overdue: Rent",
            "click": "https://money.example.com/bills/b1",
        })))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    NtfyNotifier::new(PUBLIC_URL.into())
        .send(&ntfy(&server, Some("tk_123")), &message())
        .await
        .unwrap();
}

#[tokio::test]
async fn http_errors_become_status_errors_and_redirects_are_not_followed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "https://evil.example/"))
        .mount(&server)
        .await;
    let result = NtfyNotifier::new(PUBLIC_URL.into())
        .send(&ntfy(&server, None), &message())
        .await;
    assert_eq!(result, Err(NotifyError::Status(302)));
}

#[tokio::test]
async fn unreachable_servers_fail_without_naming_the_url() {
    let channel = channel(ChannelConfig::Ntfy {
        server_url: "http://127.0.0.1:9/secret-topic".into(),
        topic: "secret-topic".into(),
        token: None,
    });
    let error = NtfyNotifier::new(PUBLIC_URL.into())
        .send(&channel, &message())
        .await
        .unwrap_err();
    assert!(matches!(error, NotifyError::Failed(_)));
    assert!(!error.to_string().contains("secret-topic"), "{error}");
}

#[tokio::test]
async fn gotify_sends_the_token_in_a_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/message"))
        .and(header("x-gotify-key", "AppToken"))
        .and(body_partial_json(serde_json::json!({
            "title": "Bill overdue: Rent",
            "priority": 5,
        })))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let gotify = channel(ChannelConfig::Gotify {
        server_url: server.uri(),
        token: "AppToken".into(),
    });
    GotifyNotifier::new(PUBLIC_URL.into())
        .send(&gotify, &message())
        .await
        .unwrap();
    // A sender refuses another kind's channel.
    let wrong = GotifyNotifier::new(PUBLIC_URL.into())
        .send(&ntfy(&server, None), &message())
        .await;
    assert_eq!(wrong, Err(NotifyError::WrongKind("Gotify")));
}

fn vapid() -> VapidConfig {
    let (private_key, public_key) = generate_vapid_keys();
    VapidConfig {
        private_key,
        public_key,
        subject: "mailto:me@example.com".into(),
    }
}

#[tokio::test]
async fn web_push_posts_an_encrypted_body_signed_with_vapid() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/push/abc"))
        .and(header("content-encoding", "aes128gcm"))
        .and(header_exists("ttl"))
        .respond_with(ResponseTemplate::new(201))
        .expect(1)
        .mount(&server)
        .await;
    let vapid = vapid();
    let push = channel(ChannelConfig::WebPush {
        endpoint: format!("{}/push/abc", server.uri()),
        p256dh: P256DH.into(),
        auth: AUTH.into(),
    });
    WebPushNotifier::new(vapid.clone())
        .send(&push, &message())
        .await
        .unwrap();
    let request = &server.received_requests().await.unwrap()[0];
    let authorization = request.headers["authorization"].to_str().unwrap();
    assert!(authorization.starts_with("vapid t="));
    assert!(authorization.ends_with(&format!("k={}", vapid.public_key)));
    // Encrypted: the title is not readable on the wire.
    assert!(!String::from_utf8_lossy(&request.body).contains("Rent"));
}

#[tokio::test]
async fn web_push_rejects_broken_subscriptions_and_reports_expired_ones() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(410))
        .mount(&server)
        .await;
    let sender = WebPushNotifier::new(vapid());
    let endpoint = format!("{}/push/gone", server.uri());
    let gone = channel(ChannelConfig::WebPush {
        endpoint: endpoint.clone(),
        p256dh: P256DH.into(),
        auth: AUTH.into(),
    });
    assert_eq!(
        sender.send(&gone, &message()).await,
        Err(NotifyError::Status(410))
    );
    let broken = channel(ChannelConfig::WebPush {
        endpoint,
        p256dh: "A".repeat(87),
        auth: AUTH.into(),
    });
    let error = sender.send(&broken, &message()).await.unwrap_err();
    assert!(error.to_string().contains("p256dh"));
}

/// A minimal SMTP server that accepts one mail and hands back its DATA.
async fn fake_smtp() -> (u16, Arc<Mutex<String>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let received = Arc::new(Mutex::new(String::new()));
    let store = received.clone();
    tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let (read, mut write) = socket.into_split();
        let mut lines = BufReader::new(read).lines();
        write.write_all(b"220 fake ESMTP\r\n").await.unwrap();
        let mut in_data = false;
        while let Ok(Some(line)) = lines.next_line().await {
            let reply: &[u8] = if in_data {
                if line == "." {
                    in_data = false;
                    b"250 queued\r\n"
                } else {
                    store.lock().unwrap().push_str(&format!("{line}\n"));
                    continue;
                }
            } else if line.starts_with("DATA") {
                in_data = true;
                b"354 go ahead\r\n"
            } else if line.starts_with("QUIT") {
                write.write_all(b"221 bye\r\n").await.unwrap();
                break;
            } else {
                b"250 ok\r\n"
            };
            write.write_all(reply).await.unwrap();
        }
    });
    (port, received)
}

#[tokio::test]
async fn email_goes_through_the_configured_smtp_server() {
    let (port, received) = fake_smtp().await;
    let smtp = SmtpConfig {
        host: "127.0.0.1".into(),
        port,
        tls: SmtpTls::None,
        credentials: None,
        from: "Saldo <saldo@example.com>".into(),
    };
    let email = channel(ChannelConfig::Email {
        to: "me@example.com".into(),
    });
    EmailNotifier::new(smtp, PUBLIC_URL.into())
        .send(&email, &message())
        .await
        .unwrap();
    let data = received.lock().unwrap().clone();
    assert!(data.contains("Subject: Bill overdue: Rent"), "{data}");
    assert!(data.contains("To: me@example.com"), "{data}");
    assert!(
        data.contains("https://money.example.com/bills/b1"),
        "{data}"
    );
}

#[tokio::test]
async fn senders_pick_by_kind_and_report_missing_configuration() {
    let config = Config::from_lookup(|_| None).unwrap();
    let senders = Senders::from_config(&config);
    assert!(senders.for_kind(ChannelKind::Ntfy).is_some());
    assert!(senders.for_kind(ChannelKind::Email).is_none());
    assert!(senders.for_kind(ChannelKind::WebPush).is_none());
    let email = channel(ChannelConfig::Email {
        to: "me@example.com".into(),
    });
    let error = senders.send(&email, &message()).await.unwrap_err();
    assert!(matches!(error, NotifyError::NotConfigured(_)));
    assert!(error.to_string().contains("SALDO_SMTP"));
}
