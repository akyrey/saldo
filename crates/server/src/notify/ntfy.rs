//! ntfy (<https://ntfy.sh>, or self-hosted): `POST <server>/<topic>`.

use async_trait::async_trait;
use saldo_app::notifications::{Channel, ChannelConfig, Message};

use super::{Notifier, NotifyError, absolute, http};

pub struct NtfyNotifier {
    public_url: String,
}

impl NtfyNotifier {
    pub fn new(public_url: String) -> Self {
        Self { public_url }
    }
}

#[async_trait]
impl Notifier for NtfyNotifier {
    async fn send(&self, channel: &Channel, message: &Message) -> Result<(), NotifyError> {
        let ChannelConfig::Ntfy {
            server_url,
            topic,
            token,
        } = &channel.config
        else {
            return Err(NotifyError::WrongKind("ntfy"));
        };
        // The JSON form keeps non-ASCII titles intact (headers cannot).
        let body = serde_json::json!({
            "topic": topic,
            "title": message.title,
            "message": message.body,
            "click": absolute(&self.public_url, &message.link),
            "tags": ["money_with_wings"],
        });
        let mut request = http::client()?.post(server_url).json(&body);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        http::send(request).await
    }
}
