//! Gotify (<https://gotify.net>): `POST <server>/message` with an
//! application token.

use async_trait::async_trait;
use saldo_app::notifications::{Channel, ChannelConfig, Message};

use super::{Notifier, NotifyError, absolute, http};

/// Gotify's default priority that still makes a sound on Android.
const PRIORITY: u8 = 5;

pub struct GotifyNotifier {
    public_url: String,
}

impl GotifyNotifier {
    pub fn new(public_url: String) -> Self {
        Self { public_url }
    }
}

#[async_trait]
impl Notifier for GotifyNotifier {
    async fn send(&self, channel: &Channel, message: &Message) -> Result<(), NotifyError> {
        let ChannelConfig::Gotify { server_url, token } = &channel.config else {
            return Err(NotifyError::WrongKind("Gotify"));
        };
        let body = serde_json::json!({
            "title": message.title,
            "message": message.body,
            "priority": PRIORITY,
            "extras": {
                "client::notification": { "click": { "url": absolute(&self.public_url, &message.link) } }
            },
        });
        // The token goes in a header, never in the URL, so it cannot leak
        // into proxy or server logs.
        let request = http::client()?
            .post(format!("{server_url}/message"))
            .header("X-Gotify-Key", token)
            .json(&body);
        http::send(request).await
    }
}
