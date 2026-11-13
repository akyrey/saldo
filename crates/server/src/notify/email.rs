//! Email through the SMTP server from `SALDO_SMTP_*` (`lettre`, rustls).

use async_trait::async_trait;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use saldo_app::notifications::{Channel, ChannelConfig, Message};

use super::config::{SmtpConfig, SmtpTls};
use super::{Notifier, NotifyError, absolute};

pub struct EmailNotifier {
    smtp: SmtpConfig,
    public_url: String,
}

impl EmailNotifier {
    pub fn new(smtp: SmtpConfig, public_url: String) -> Self {
        Self { smtp, public_url }
    }

    /// Builds the mail. Addresses were validated on input; they are parsed
    /// again here because `lettre` wants its own types.
    pub fn email(&self, to: &str, message: &Message) -> Result<lettre::Message, NotifyError> {
        let from: Mailbox = parse(&self.smtp.from)?;
        let to: Mailbox = parse(to)?;
        let text = format!(
            "{}\n\n{}\n\n-- \nSaldo",
            message.body,
            absolute(&self.public_url, &message.link)
        );
        lettre::Message::builder()
            .from(from)
            .to(to)
            .subject(&message.title)
            .body(text)
            .map_err(|e| NotifyError::Failed(format!("cannot build the email: {e}")))
    }

    /// A transport per send: reminders are rare, and building it cannot
    /// then fail at startup after the settings were checked.
    fn transport(&self) -> Result<AsyncSmtpTransport<Tokio1Executor>, NotifyError> {
        let smtp = &self.smtp;
        let builder = match smtp.tls {
            SmtpTls::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp.host),
            SmtpTls::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp.host),
            SmtpTls::None => Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(
                &smtp.host,
            )),
        }
        .map_err(|e| NotifyError::Failed(format!("SMTP setup failed: {e}")))?;
        let builder = builder.port(smtp.port);
        let builder = match &smtp.credentials {
            Some((user, password)) => {
                builder.credentials(Credentials::new(user.clone(), password.clone()))
            }
            None => builder,
        };
        Ok(builder.build())
    }
}

#[async_trait]
impl Notifier for EmailNotifier {
    async fn send(&self, channel: &Channel, message: &Message) -> Result<(), NotifyError> {
        let ChannelConfig::Email { to } = &channel.config else {
            return Err(NotifyError::WrongKind("email"));
        };
        let email = self.email(to, message)?;
        self.transport()?
            .send(email)
            .await
            .map(|_| ())
            // lettre's errors name the server's answer, never the password.
            .map_err(|e| NotifyError::Failed(format!("SMTP: {e}")))
    }
}

fn parse(address: &str) -> Result<Mailbox, NotifyError> {
    address
        .parse()
        .map_err(|_| NotifyError::Failed(format!("invalid email address {address:?}")))
}
