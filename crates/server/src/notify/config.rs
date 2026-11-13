//! SMTP and VAPID settings from `SALDO_SMTP_*` and `SALDO_VAPID_*`. Both are
//! optional: without them, email or Web Push channels cannot be created.

use std::fmt;

use base64ct::{Base64UrlUnpadded, Encoding};
use lettre::message::Mailbox;
use web_push_native::jwt_simple::algorithms::{ECDSAP256PublicKeyLike, ES256KeyPair};

use crate::config::ConfigError;

/// Length of a raw P-256 private key.
const VAPID_KEY_BYTES: usize = 32;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotifyConfig {
    pub smtp: Option<SmtpConfig>,
    pub vapid: Option<VapidConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtpTls {
    /// Plain connection upgraded with STARTTLS (port 587).
    StartTls,
    /// TLS from the first byte (port 465).
    Tls,
    /// No encryption: only for a relay on the same host or LAN.
    None,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub tls: SmtpTls,
    /// User name and password, when the server needs a login.
    pub credentials: Option<(String, String)>,
    pub from: String,
}

/// Written by hand so the password never reaches a log line.
impl fmt::Debug for SmtpConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SmtpConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("tls", &self.tls)
            .field("user", &self.credentials.as_ref().map(|(user, _)| user))
            .field("from", &self.from)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct VapidConfig {
    /// The raw private key, base64url (`SALDO_VAPID_PRIVATE_KEY`). Secret.
    pub private_key: String,
    /// The matching public key, base64url: what browsers subscribe with.
    pub public_key: String,
    /// `mailto:` or `https:` contact sent to push services.
    pub subject: String,
}

impl fmt::Debug for VapidConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VapidConfig")
            .field("public_key", &self.public_key)
            .field("subject", &self.subject)
            .finish_non_exhaustive()
    }
}

/// Reads the settings; `public_url` is the default VAPID subject.
pub fn from_lookup(
    lookup: &impl Fn(&str) -> Option<String>,
    public_url: &str,
) -> Result<NotifyConfig, ConfigError> {
    let get = |name: &str| {
        lookup(name)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    Ok(NotifyConfig {
        smtp: smtp(&get)?,
        vapid: vapid(&get, public_url)?,
    })
}

fn smtp(get: &impl Fn(&str) -> Option<String>) -> Result<Option<SmtpConfig>, ConfigError> {
    let Some(host) = get("SALDO_SMTP_HOST") else {
        for other in ["SALDO_SMTP_FROM", "SALDO_SMTP_USER", "SALDO_SMTP_PASSWORD"] {
            if get(other).is_some() {
                return Err(error(other, "is set but SALDO_SMTP_HOST is not"));
            }
        }
        return Ok(None);
    };
    let tls = match get("SALDO_SMTP_TLS").as_deref() {
        None | Some("starttls") => SmtpTls::StartTls,
        Some("tls") => SmtpTls::Tls,
        Some("none") => SmtpTls::None,
        Some(_) => return Err(error("SALDO_SMTP_TLS", "must be starttls, tls or none")),
    };
    let default_port = match tls {
        SmtpTls::StartTls => 587,
        SmtpTls::Tls => 465,
        SmtpTls::None => 25,
    };
    let port = match get("SALDO_SMTP_PORT") {
        Some(text) => text
            .parse()
            .map_err(|_| error("SALDO_SMTP_PORT", "must be a port number"))?,
        None => default_port,
    };
    let credentials = match (get("SALDO_SMTP_USER"), get("SALDO_SMTP_PASSWORD")) {
        (Some(user), Some(password)) => Some((user, password)),
        (None, None) => None,
        (Some(_), None) => {
            return Err(error(
                "SALDO_SMTP_PASSWORD",
                "is required with SALDO_SMTP_USER",
            ));
        }
        (None, Some(_)) => {
            return Err(error(
                "SALDO_SMTP_USER",
                "is required with SALDO_SMTP_PASSWORD",
            ));
        }
    };
    let from = get("SALDO_SMTP_FROM")
        .ok_or_else(|| error("SALDO_SMTP_FROM", "is required with SALDO_SMTP_HOST"))?;
    from.parse::<Mailbox>().map_err(|_| {
        error(
            "SALDO_SMTP_FROM",
            "must be an address like Saldo <saldo@example.com>",
        )
    })?;
    Ok(Some(SmtpConfig {
        host,
        port,
        tls,
        credentials,
        from,
    }))
}

fn vapid(
    get: &impl Fn(&str) -> Option<String>,
    public_url: &str,
) -> Result<Option<VapidConfig>, ConfigError> {
    let Some(private_key) = get("SALDO_VAPID_PRIVATE_KEY") else {
        return Ok(None);
    };
    let private_key = private_key.trim_end_matches('=').to_string();
    let public_key = public_key_of(&private_key).ok_or_else(|| {
        error(
            "SALDO_VAPID_PRIVATE_KEY",
            "must be a base64url P-256 private key (see `saldo vapid-keys`)",
        )
    })?;
    let subject = match get("SALDO_VAPID_SUBJECT") {
        Some(subject) => subject,
        None if public_url.starts_with("https://") => public_url.to_string(),
        None => {
            return Err(error(
                "SALDO_VAPID_SUBJECT",
                "is required when SALDO_PUBLIC_URL is not https (use mailto:you@example.com)",
            ));
        }
    };
    if !subject.starts_with("mailto:") && !subject.starts_with("https://") {
        return Err(error(
            "SALDO_VAPID_SUBJECT",
            "must start with mailto: or https://",
        ));
    }
    Ok(Some(VapidConfig {
        private_key,
        public_key,
        subject,
    }))
}

/// The VAPID key pair from its base64url private key, if valid.
pub fn key_pair(private_key: &str) -> Option<ES256KeyPair> {
    let bytes = Base64UrlUnpadded::decode_vec(private_key).ok()?;
    // `from_bytes` panics on a wrong length, so check it first.
    if bytes.len() != VAPID_KEY_BYTES {
        return None;
    }
    ES256KeyPair::from_bytes(&bytes).ok()
}

fn public_key_of(private_key: &str) -> Option<String> {
    let pair = key_pair(private_key)?;
    let bytes = pair.public_key().public_key().to_bytes_uncompressed();
    Some(Base64UrlUnpadded::encode_string(&bytes))
}

/// A new VAPID key pair `(private, public)`, base64url (`saldo vapid-keys`).
pub fn generate_vapid_keys() -> (String, String) {
    let pair = ES256KeyPair::generate();
    let private_key = Base64UrlUnpadded::encode_string(&pair.to_bytes());
    let public_key =
        Base64UrlUnpadded::encode_string(&pair.public_key().public_key().to_bytes_uncompressed());
    (private_key, public_key)
}

fn error(variable: &'static str, message: &str) -> ConfigError {
    ConfigError {
        variable,
        message: message.to_string(),
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
