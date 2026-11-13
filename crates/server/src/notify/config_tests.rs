use std::collections::HashMap;

use super::*;

/// The private key from the `web-push-native` documentation.
const PRIVATE: &str = "RS0WdYWWo1HajXg3NZR1olzCf31i-ZBGDkFyCs7j1jw";

fn read(vars: &[(&str, &str)], public_url: &str) -> Result<NotifyConfig, ConfigError> {
    let map: HashMap<String, String> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    from_lookup(&|name: &str| map.get(name).cloned(), public_url)
}

#[test]
fn nothing_set_means_neither_email_nor_push() {
    assert_eq!(
        read(&[], "http://localhost").unwrap(),
        NotifyConfig::default()
    );
}

#[test]
fn smtp_defaults_to_starttls_on_587() {
    let config = read(
        &[
            ("SALDO_SMTP_HOST", "smtp.example.com"),
            ("SALDO_SMTP_FROM", "Saldo <saldo@example.com>"),
            ("SALDO_SMTP_USER", "saldo"),
            ("SALDO_SMTP_PASSWORD", "hunter2"),
        ],
        "http://localhost",
    )
    .unwrap();
    let smtp = config.smtp.unwrap();
    assert_eq!((smtp.port, smtp.tls), (587, SmtpTls::StartTls));
    assert!(smtp.credentials.is_some());
    // Debug output is safe to log.
    let shown = format!("{smtp:?}");
    assert!(
        shown.contains("saldo") && !shown.contains("hunter2"),
        "{shown}"
    );
}

#[test]
fn smtp_tls_modes_pick_their_ports() {
    for (mode, port, tls) in [("tls", 465, SmtpTls::Tls), ("none", 25, SmtpTls::None)] {
        let config = read(
            &[
                ("SALDO_SMTP_HOST", "mail.lan"),
                ("SALDO_SMTP_FROM", "saldo@example.com"),
                ("SALDO_SMTP_TLS", mode),
            ],
            "http://localhost",
        )
        .unwrap();
        let smtp = config.smtp.unwrap();
        assert_eq!((smtp.port, smtp.tls, smtp.credentials), (port, tls, None));
    }
}

#[test]
fn smtp_errors_name_the_variable() {
    let host = ("SALDO_SMTP_HOST", "smtp.example.com");
    let from = ("SALDO_SMTP_FROM", "saldo@example.com");
    let cases: Vec<(Vec<(&str, &str)>, &str)> = vec![
        (vec![from], "SALDO_SMTP_FROM"),
        (vec![host], "SALDO_SMTP_FROM"),
        (
            vec![host, ("SALDO_SMTP_FROM", "not an address")],
            "SALDO_SMTP_FROM",
        ),
        (
            vec![host, from, ("SALDO_SMTP_PORT", "x")],
            "SALDO_SMTP_PORT",
        ),
        (
            vec![host, from, ("SALDO_SMTP_TLS", "ssl")],
            "SALDO_SMTP_TLS",
        ),
        (
            vec![host, from, ("SALDO_SMTP_USER", "u")],
            "SALDO_SMTP_PASSWORD",
        ),
        (
            vec![host, from, ("SALDO_SMTP_PASSWORD", "p")],
            "SALDO_SMTP_USER",
        ),
    ];
    for (vars, variable) in cases {
        let err = read(&vars, "http://localhost").unwrap_err();
        assert_eq!(err.variable, variable, "{vars:?}");
    }
}

#[test]
fn vapid_derives_the_public_key_and_defaults_the_subject_to_an_https_url() {
    let config = read(
        &[("SALDO_VAPID_PRIVATE_KEY", PRIVATE)],
        "https://money.example.com",
    )
    .unwrap();
    let vapid = config.vapid.unwrap();
    assert_eq!(vapid.subject, "https://money.example.com");
    // 65-byte uncompressed point, base64url without padding.
    assert_eq!(vapid.public_key.len(), 87);
    assert!(vapid.public_key.starts_with('B'));
    assert!(!format!("{vapid:?}").contains(PRIVATE));
}

#[test]
fn vapid_errors_name_the_variable() {
    let cases = [
        (
            vec![("SALDO_VAPID_PRIVATE_KEY", "short")],
            "https://x.example",
            "SALDO_VAPID_PRIVATE_KEY",
        ),
        (
            vec![("SALDO_VAPID_PRIVATE_KEY", PRIVATE)],
            "http://localhost",
            "SALDO_VAPID_SUBJECT",
        ),
        (
            vec![
                ("SALDO_VAPID_PRIVATE_KEY", PRIVATE),
                ("SALDO_VAPID_SUBJECT", "me@example.com"),
            ],
            "http://localhost",
            "SALDO_VAPID_SUBJECT",
        ),
    ];
    for (vars, public_url, variable) in cases {
        assert_eq!(read(&vars, public_url).unwrap_err().variable, variable);
    }
    let mailto = read(
        &[
            ("SALDO_VAPID_PRIVATE_KEY", PRIVATE),
            ("SALDO_VAPID_SUBJECT", "mailto:me@example.com"),
        ],
        "http://localhost",
    )
    .unwrap();
    assert_eq!(mailto.vapid.unwrap().subject, "mailto:me@example.com");
}

#[test]
fn generated_keys_are_accepted() {
    let (private_key, public_key) = generate_vapid_keys();
    assert_eq!(public_key_of(&private_key), Some(public_key));
    assert!(key_pair("not base64!").is_none());
}
