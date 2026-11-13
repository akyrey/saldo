//! Web Push with VAPID (RFC 8030, 8291, 8292), through `web-push-native`
//! (pure Rust crypto) and our own `reqwest` client.

use async_trait::async_trait;
use base64ct::{Base64UrlUnpadded, Encoding};
use saldo_app::notifications::{Channel, ChannelConfig, Message};
use web_push_native::p256::PublicKey;
use web_push_native::{Auth, WebPushBuilder};

use super::config::{VapidConfig, key_pair};
use super::{Notifier, NotifyError, http};

/// Length of the subscription's auth secret.
const AUTH_BYTES: usize = 16;

pub struct WebPushNotifier {
    vapid: VapidConfig,
}

impl WebPushNotifier {
    pub fn new(vapid: VapidConfig) -> Self {
        Self { vapid }
    }
}

/// What the service worker receives (`web/public/push-sw.js`).
fn payload(message: &Message) -> String {
    serde_json::json!({
        "title": message.title,
        "body": message.body,
        "url": message.link,
        "tag": message.tag,
    })
    .to_string()
}

fn invalid(what: &str) -> NotifyError {
    NotifyError::Failed(format!("the push subscription has an invalid {what}"))
}

#[async_trait]
impl Notifier for WebPushNotifier {
    async fn send(&self, channel: &Channel, message: &Message) -> Result<(), NotifyError> {
        let ChannelConfig::WebPush {
            endpoint,
            p256dh,
            auth,
        } = &channel.config
        else {
            return Err(NotifyError::WrongKind("Web Push"));
        };
        let keys = key_pair(&self.vapid.private_key).ok_or(NotifyError::NotConfigured(
            "Web Push (SALDO_VAPID_PRIVATE_KEY)",
        ))?;
        let public = Base64UrlUnpadded::decode_vec(p256dh)
            .ok()
            .and_then(|bytes| PublicKey::from_sec1_bytes(&bytes).ok())
            .ok_or_else(|| invalid("p256dh key"))?;
        let auth: [u8; AUTH_BYTES] = Base64UrlUnpadded::decode_vec(auth)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| invalid("auth secret"))?;
        let request = WebPushBuilder::new(
            endpoint.parse().map_err(|_| invalid("endpoint"))?,
            public,
            Auth::from(auth),
        )
        .with_vapid(&keys, &self.vapid.subject)
        .build(payload(message))
        .map_err(|e| NotifyError::Failed(format!("cannot encrypt the push message: {e}")))?;
        // Copy the prepared request (encrypted body, VAPID and TTL headers)
        // onto our client, which has the timeout and no redirects.
        let (parts, body) = request.into_parts();
        let request = http::client()?
            .post(endpoint)
            .headers(parts.headers)
            .body(body);
        http::send(request).await
    }
}
