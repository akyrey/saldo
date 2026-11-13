//! Notification channel kinds and their settings, validated at the boundary.
//!
//! The settings are stored as JSON in `notification_channels.config`. Tokens
//! and push keys are secrets: they are stored, used to send, and never
//! returned by the API (see [`ChannelSettings`]).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::AppError;

/// The four channel kinds of ARCHITECTURE.md 8.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelKind {
    Ntfy,
    Gotify,
    Email,
    WebPush,
}

impl ChannelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ChannelKind::Ntfy => "ntfy",
            ChannelKind::Gotify => "gotify",
            ChannelKind::Email => "email",
            ChannelKind::WebPush => "webpush",
        }
    }

    pub fn parse(text: &str) -> Result<Self, AppError> {
        match text {
            "ntfy" => Ok(ChannelKind::Ntfy),
            "gotify" => Ok(ChannelKind::Gotify),
            "email" => Ok(ChannelKind::Email),
            "webpush" => Ok(ChannelKind::WebPush),
            _ => Err(AppError::invalid(
                "kind",
                "must be ntfy, gotify, email or webpush",
            )),
        }
    }
}

/// A channel's settings. An enum whose variants carry data, like a Go
/// interface implemented by four structs, but `match` must handle every
/// variant, so adding a kind shows every place that needs updating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelConfig {
    /// POST to `<server_url>/<topic>`, with an optional access token.
    Ntfy {
        server_url: String,
        topic: String,
        token: Option<String>,
    },
    /// POST to `<server_url>/message` with an application token.
    Gotify { server_url: String, token: String },
    /// Mail to `to`, through the server's SMTP settings.
    Email { to: String },
    /// A browser push subscription (`PushSubscription.toJSON()`).
    WebPush {
        endpoint: String,
        p256dh: String,
        auth: String,
    },
}

// The stored JSON shapes, one per kind. Unknown fields (such as the
// browser's `expirationTime`) are ignored and not stored.
#[derive(Serialize, Deserialize)]
struct NtfyJson {
    server_url: String,
    topic: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    token: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct GotifyJson {
    server_url: String,
    token: String,
}

#[derive(Serialize, Deserialize)]
struct EmailJson {
    to: String,
}

#[derive(Serialize, Deserialize)]
struct PushKeys {
    p256dh: String,
    auth: String,
}

#[derive(Serialize, Deserialize)]
struct WebPushJson {
    endpoint: String,
    keys: PushKeys,
}

impl ChannelConfig {
    pub fn kind(&self) -> ChannelKind {
        match self {
            ChannelConfig::Ntfy { .. } => ChannelKind::Ntfy,
            ChannelConfig::Gotify { .. } => ChannelKind::Gotify,
            ChannelConfig::Email { .. } => ChannelKind::Email,
            ChannelConfig::WebPush { .. } => ChannelKind::WebPush,
        }
    }

    /// Reads and validates settings sent by a client.
    pub fn parse(kind: ChannelKind, value: &Value) -> Result<Self, AppError> {
        let config = match kind {
            ChannelKind::Ntfy => {
                let json: NtfyJson = shape(value)?;
                ChannelConfig::Ntfy {
                    server_url: http_url("config.server_url", &json.server_url)?,
                    topic: topic(&json.topic)?,
                    token: optional_secret(json.token),
                }
            }
            ChannelKind::Gotify => {
                let json: GotifyJson = shape(value)?;
                ChannelConfig::Gotify {
                    server_url: http_url("config.server_url", &json.server_url)?,
                    token: secret("config.token", json.token)?,
                }
            }
            ChannelKind::Email => ChannelConfig::Email {
                to: email(&shape::<EmailJson>(value)?.to)?,
            },
            ChannelKind::WebPush => web_push(shape(value)?)?,
        };
        Ok(config)
    }

    /// Reads settings stored by [`Self::to_json`]; a failure means a corrupt row.
    pub fn from_stored(kind: &str, text: &str) -> Result<Self, AppError> {
        let corrupt = |_| AppError::Internal("corrupt notification channel config".to_string());
        let value: Value = serde_json::from_str(text).map_err(corrupt)?;
        Self::parse(ChannelKind::parse(kind)?, &value)
            .map_err(|_| AppError::Internal("corrupt notification channel config".to_string()))
    }

    /// The JSON stored in the database.
    pub fn to_json(&self) -> Value {
        let value = match self.clone() {
            ChannelConfig::Ntfy {
                server_url,
                topic,
                token,
            } => serde_json::to_value(NtfyJson {
                server_url,
                topic,
                token,
            }),
            ChannelConfig::Gotify { server_url, token } => {
                serde_json::to_value(GotifyJson { server_url, token })
            }
            ChannelConfig::Email { to } => serde_json::to_value(EmailJson { to }),
            ChannelConfig::WebPush {
                endpoint,
                p256dh,
                auth,
            } => serde_json::to_value(WebPushJson {
                endpoint,
                keys: PushKeys { p256dh, auth },
            }),
        };
        // Serialising plain strings cannot fail.
        value.unwrap_or(Value::Null)
    }

    /// A PATCH: the fields in `patch` replace the stored ones (`null` removes
    /// one), then the result is validated again. So a client can change the
    /// topic without sending the token it never sees.
    pub fn merged(&self, patch: &Value) -> Result<Self, AppError> {
        let Value::Object(changes) = patch else {
            return Err(AppError::invalid("config", "must be an object"));
        };
        let mut merged: Map<String, Value> = match self.to_json() {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        for (key, value) in changes {
            if value.is_null() {
                merged.remove(key);
            } else {
                merged.insert(key.clone(), value.clone());
            }
        }
        Self::parse(self.kind(), &Value::Object(merged))
    }

    /// What the API shows: everything but the secrets.
    pub fn settings(&self) -> ChannelSettings {
        let mut settings = ChannelSettings::default();
        match self {
            ChannelConfig::Ntfy {
                server_url,
                topic,
                token,
            } => {
                settings.server_url = Some(server_url.clone());
                settings.topic = Some(topic.clone());
                settings.has_token = token.is_some();
            }
            ChannelConfig::Gotify { server_url, .. } => {
                settings.server_url = Some(server_url.clone());
                settings.has_token = true;
            }
            ChannelConfig::Email { to } => settings.to = Some(to.clone()),
            ChannelConfig::WebPush { endpoint, .. } => {
                settings.push_service = origin(endpoint);
            }
        }
        settings
    }
}

/// A channel's settings without secrets, as the API returns them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct ChannelSettings {
    /// ntfy and Gotify: the server.
    pub server_url: Option<String>,
    /// ntfy: the topic.
    pub topic: Option<String>,
    /// email: the recipient.
    pub to: Option<String>,
    /// webpush: the browser vendor's push service (scheme and host only).
    pub push_service: Option<String>,
    /// Whether an access token is stored (never shown).
    pub has_token: bool,
}

fn shape<T: for<'de> Deserialize<'de>>(value: &Value) -> Result<T, AppError> {
    serde_json::from_value(value.clone()).map_err(|e| AppError::invalid("config", e.to_string()))
}

const MAX_URL: usize = 2048;
const MAX_TOPIC: usize = 64;
const MAX_SECRET: usize = 512;
const MAX_EMAIL: usize = 254;
/// Base64url length of a 65-byte uncompressed P-256 point, without padding.
const P256DH_LENGTH: usize = 87;
/// Base64url length of the 16-byte push auth secret, without padding.
const AUTH_LENGTH: usize = 22;

fn http_url(field: &str, text: &str) -> Result<String, AppError> {
    let url = text.trim().trim_end_matches('/');
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    let host = rest.and_then(|r| r.split(['/', '?', '#']).next());
    let valid = host.is_some_and(|h| !h.is_empty() && !h.contains(char::is_whitespace))
        && !url.contains(['?', '#'])
        && url.len() <= MAX_URL;
    if valid {
        Ok(url.to_string())
    } else {
        Err(AppError::invalid(
            field,
            "must be an http:// or https:// URL without query",
        ))
    }
}

fn topic(text: &str) -> Result<String, AppError> {
    let valid = !text.is_empty()
        && text.len() <= MAX_TOPIC
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if valid {
        Ok(text.to_string())
    } else {
        Err(AppError::invalid(
            "config.topic",
            "must be 1 to 64 letters, digits, - or _",
        ))
    }
}

fn secret(field: &str, text: String) -> Result<String, AppError> {
    let text = text.trim().to_string();
    if text.is_empty() || text.len() > MAX_SECRET || text.contains(char::is_control) {
        return Err(AppError::invalid(
            field,
            "must be a non-empty token of at most 512 characters",
        ));
    }
    Ok(text)
}

fn optional_secret(text: Option<String>) -> Option<String> {
    text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

fn email(text: &str) -> Result<String, AppError> {
    let text = text.trim();
    let valid = text.len() <= MAX_EMAIL
        && !text.contains(|c: char| c.is_whitespace() || c.is_control() || c == '<' || c == '>')
        && text.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.contains('@')
        });
    if valid {
        Ok(text.to_string())
    } else {
        Err(AppError::invalid(
            "config.to",
            "must be an email address like me@example.com",
        ))
    }
}

fn web_push(json: WebPushJson) -> Result<ChannelConfig, AppError> {
    let endpoint = json.endpoint.trim().to_string();
    if !endpoint.starts_with("https://") || endpoint.len() > MAX_URL || origin(&endpoint).is_none()
    {
        return Err(AppError::invalid(
            "config.endpoint",
            "must be an https:// URL",
        ));
    }
    Ok(ChannelConfig::WebPush {
        endpoint,
        p256dh: base64url("config.keys.p256dh", &json.keys.p256dh, P256DH_LENGTH)?,
        auth: base64url("config.keys.auth", &json.keys.auth, AUTH_LENGTH)?,
    })
}

/// Checks the shape of a base64url key without decoding it (the sender
/// decodes); padding is removed.
fn base64url(field: &str, text: &str, length: usize) -> Result<String, AppError> {
    let text = text.trim().trim_end_matches('=');
    let valid = text.len() == length
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if valid {
        Ok(text.to_string())
    } else {
        Err(AppError::invalid(
            field,
            "is not a key from PushSubscription.toJSON()",
        ))
    }
}

/// `https://push.example.com/abc` → `https://push.example.com`.
fn origin(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .filter(|h| !h.is_empty())?;
    Some(format!("{scheme}://{host}"))
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
