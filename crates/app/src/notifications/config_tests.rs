use serde_json::json;

use super::*;

const P256DH: &str =
    "BLn9b-VR0ca83knDNZ32dCHGyjJp-1riX9ZTN40MqV8K_LpQmLqxC_DoHvqvFXO_nGdAB4W9dogZb_sM-uV4JbY";
const AUTH: &str = "_ordMnz7uTCmrpBTeUV4Bw";

fn field_of(error: AppError) -> String {
    match error {
        AppError::Invalid { field, .. } => field,
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn kinds_round_trip_through_text() {
    for kind in [
        ChannelKind::Ntfy,
        ChannelKind::Gotify,
        ChannelKind::Email,
        ChannelKind::WebPush,
    ] {
        assert_eq!(ChannelKind::parse(kind.as_str()).unwrap(), kind);
    }
    assert_eq!(field_of(ChannelKind::parse("sms").unwrap_err()), "kind");
}

#[test]
fn ntfy_settings_are_normalised_and_the_token_is_optional() {
    let config = ChannelConfig::parse(
        ChannelKind::Ntfy,
        &json!({ "server_url": " https://ntfy.sh/ ", "topic": "saldo-bills", "token": "  " }),
    )
    .unwrap();
    assert_eq!(
        config,
        ChannelConfig::Ntfy {
            server_url: "https://ntfy.sh".into(),
            topic: "saldo-bills".into(),
            token: None
        }
    );
    assert_eq!(config.kind(), ChannelKind::Ntfy);
    assert!(!config.settings().has_token);
}

#[test]
fn invalid_fields_are_named() {
    let cases = [
        (
            ChannelKind::Ntfy,
            json!({ "server_url": "ftp://x", "topic": "a" }),
            "config.server_url",
        ),
        (
            ChannelKind::Ntfy,
            json!({ "server_url": "https://x?y=1", "topic": "a" }),
            "config.server_url",
        ),
        (
            ChannelKind::Ntfy,
            json!({ "server_url": "https://x", "topic": "a b" }),
            "config.topic",
        ),
        (ChannelKind::Ntfy, json!({ "topic": "a" }), "config"),
        (
            ChannelKind::Gotify,
            json!({ "server_url": "http://gotify.lan", "token": "" }),
            "config.token",
        ),
        (
            ChannelKind::Email,
            json!({ "to": "not-an-address" }),
            "config.to",
        ),
        (
            ChannelKind::Email,
            json!({ "to": "a@b.c\r\nBcc: x@y.z" }),
            "config.to",
        ),
        (
            ChannelKind::WebPush,
            json!({ "endpoint": "http://push.example.com/x", "keys": { "p256dh": P256DH, "auth": AUTH } }),
            "config.endpoint",
        ),
        (
            ChannelKind::WebPush,
            json!({ "endpoint": "https://push.example.com/x", "keys": { "p256dh": "short", "auth": AUTH } }),
            "config.keys.p256dh",
        ),
        (
            ChannelKind::WebPush,
            json!({ "endpoint": "https://push.example.com/x", "keys": { "p256dh": P256DH, "auth": "a+b/" } }),
            "config.keys.auth",
        ),
    ];
    for (kind, value, field) in cases {
        let error = ChannelConfig::parse(kind, &value).unwrap_err();
        assert_eq!(field_of(error), field, "{value}");
    }
}

#[test]
fn settings_never_contain_secrets() {
    let gotify = ChannelConfig::parse(
        ChannelKind::Gotify,
        &json!({ "server_url": "http://gotify.lan", "token": "AbCsecret" }),
    )
    .unwrap();
    let push = ChannelConfig::parse(
        ChannelKind::WebPush,
        &json!({ "endpoint": "https://fcm.googleapis.com/fcm/send/abc", "expirationTime": null,
                 "keys": { "p256dh": P256DH, "auth": format!("{AUTH}==") } }),
    )
    .unwrap();
    let email =
        ChannelConfig::parse(ChannelKind::Email, &json!({ "to": "me@example.com" })).unwrap();
    for config in [&gotify, &push, &email] {
        let shown = serde_json::to_string(&config.settings()).unwrap();
        for secret in ["AbCsecret", P256DH, AUTH, "/fcm/send/abc"] {
            assert!(!shown.contains(secret), "{shown}");
        }
    }
    assert!(gotify.settings().has_token);
    assert_eq!(
        push.settings().push_service.as_deref(),
        Some("https://fcm.googleapis.com")
    );
    assert_eq!(email.settings().to.as_deref(), Some("me@example.com"));
}

#[test]
fn stored_json_round_trips() {
    let push = ChannelConfig::WebPush {
        endpoint: "https://push.example.com/x".into(),
        p256dh: P256DH.into(),
        auth: AUTH.into(),
    };
    let ntfy = ChannelConfig::Ntfy {
        server_url: "https://ntfy.sh".into(),
        topic: "t".into(),
        token: Some("tk".into()),
    };
    for config in [push, ntfy] {
        let text = config.to_json().to_string();
        let back = ChannelConfig::from_stored(config.kind().as_str(), &text).unwrap();
        assert_eq!(back, config);
    }
    assert!(matches!(
        ChannelConfig::from_stored("email", "{}"),
        Err(AppError::Internal(_))
    ));
    assert!(matches!(
        ChannelConfig::from_stored("email", "nope"),
        Err(AppError::Internal(_))
    ));
}

#[test]
fn a_patch_keeps_unsent_secrets_and_null_removes_them() {
    let ntfy = ChannelConfig::Ntfy {
        server_url: "https://ntfy.sh".into(),
        topic: "old".into(),
        token: Some("tk".into()),
    };
    let renamed = ntfy.merged(&json!({ "topic": "new" })).unwrap();
    assert_eq!(
        renamed,
        ChannelConfig::Ntfy {
            server_url: "https://ntfy.sh".into(),
            topic: "new".into(),
            token: Some("tk".into())
        }
    );
    let cleared = ntfy.merged(&json!({ "token": null })).unwrap();
    assert!(!cleared.settings().has_token);
    assert_eq!(field_of(ntfy.merged(&json!("x")).unwrap_err()), "config");
    assert_eq!(
        field_of(ntfy.merged(&json!({ "topic": "" })).unwrap_err()),
        "config.topic"
    );
}
