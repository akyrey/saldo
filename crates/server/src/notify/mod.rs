//! Sending notifications: one [`Notifier`] per channel kind (ARCHITECTURE.md
//! 8.4). The notifier task and the `test` endpoint pick the sender by the
//! channel's kind through [`Senders`].

pub mod config;
mod email;
mod gotify;
mod http;
mod ntfy;
mod webpush;

use std::time::Duration;

use async_trait::async_trait;
use saldo_app::notifications::{Channel, ChannelKind, Message};
use thiserror::Error;

pub use email::EmailNotifier;
pub use gotify::GotifyNotifier;
pub use ntfy::NtfyNotifier;
pub use webpush::WebPushNotifier;

use crate::config::Config;

/// Longest a single send may take, so one hanging server cannot hold up
/// the others.
pub const SEND_TIMEOUT: Duration = Duration::from_secs(15);

/// Why a send failed. Messages never contain tokens, passwords or push URLs.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum NotifyError {
    #[error("{0} is not configured on this server")]
    NotConfigured(&'static str),
    #[error("the channel is not a {0} channel")]
    WrongKind(&'static str),
    #[error("the server answered HTTP {0}")]
    Status(u16),
    /// The push service no longer knows the subscription (HTTP 404 or 410);
    /// retrying cannot help.
    #[error("the push subscription no longer exists (HTTP {0})")]
    SubscriptionGone(u16),
    #[error("{0}")]
    Failed(String),
    #[error("timed out")]
    Timeout,
}

/// One way of delivering a message: a Go interface with one implementation
/// per channel kind, or a Laravel notification channel.
///
/// Rust's built-in `async fn` in traits cannot (yet) be called through a
/// `Box<dyn Notifier>`; the `async_trait` attribute rewrites each method to
/// return a boxed future, which can.
#[async_trait]
pub trait Notifier: Send + Sync {
    async fn send(&self, channel: &Channel, message: &Message) -> Result<(), NotifyError>;
}

/// The sender for every channel kind. Email and Web Push are `None` when the
/// server has no SMTP or VAPID settings.
pub struct Senders {
    pub ntfy: Box<dyn Notifier>,
    pub gotify: Box<dyn Notifier>,
    pub email: Option<Box<dyn Notifier>>,
    pub webpush: Option<Box<dyn Notifier>>,
}

impl Senders {
    /// The real senders. Links in messages point at `SALDO_PUBLIC_URL`.
    pub fn from_config(config: &Config) -> Self {
        let base = config.public_url.clone();
        let notify = &config.notify;
        Self {
            ntfy: Box::new(NtfyNotifier::new(base.clone())),
            gotify: Box::new(GotifyNotifier::new(base.clone())),
            email: notify
                .smtp
                .clone()
                .map(|smtp| Box::new(EmailNotifier::new(smtp, base.clone())) as Box<dyn Notifier>),
            webpush: notify
                .vapid
                .clone()
                .map(|vapid| Box::new(WebPushNotifier::new(vapid)) as Box<dyn Notifier>),
        }
    }

    /// The sender for a kind; `None` if the server is not set up for it.
    pub fn for_kind(&self, kind: ChannelKind) -> Option<&dyn Notifier> {
        match kind {
            ChannelKind::Ntfy => Some(self.ntfy.as_ref()),
            ChannelKind::Gotify => Some(self.gotify.as_ref()),
            ChannelKind::Email => self.email.as_deref(),
            ChannelKind::WebPush => self.webpush.as_deref(),
        }
    }

    /// Sends through the channel's sender, giving up after [`SEND_TIMEOUT`].
    pub async fn send(&self, channel: &Channel, message: &Message) -> Result<(), NotifyError> {
        let kind = channel.config.kind();
        let sender = self
            .for_kind(kind)
            .ok_or(NotifyError::NotConfigured(kind_name(kind)))?;
        tokio::time::timeout(SEND_TIMEOUT, sender.send(channel, message))
            .await
            .map_err(|_| NotifyError::Timeout)?
    }
}

/// How the kind is named in error messages.
pub fn kind_name(kind: ChannelKind) -> &'static str {
    match kind {
        ChannelKind::Ntfy => "ntfy",
        ChannelKind::Gotify => "Gotify",
        ChannelKind::Email => "email (SALDO_SMTP_*)",
        ChannelKind::WebPush => "Web Push (SALDO_VAPID_PRIVATE_KEY)",
    }
}

/// `https://money.example.com` + `/bills/1`.
fn absolute(base: &str, link: &str) -> String {
    format!("{}{link}", base.trim_end_matches('/'))
}
