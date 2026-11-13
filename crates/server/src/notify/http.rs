//! The HTTP client shared by the ntfy, Gotify and Web Push senders.

use std::time::Duration;

use reqwest::redirect::Policy;

use super::NotifyError;

const TIMEOUT: Duration = Duration::from_secs(10);

/// A client with a 10 second timeout that never follows redirects (a
/// redirect could carry the token to another host).
pub fn client() -> Result<reqwest::Client, NotifyError> {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .redirect(Policy::none())
        .user_agent(concat!("Saldo/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| NotifyError::Failed(format!("cannot build the HTTP client: {e}")))
}

/// Sends and turns anything but a 2xx answer into an error. The URL is
/// removed from transport errors: push URLs and topics are secrets.
pub async fn send(request: reqwest::RequestBuilder) -> Result<(), NotifyError> {
    let response = request.send().await.map_err(|e| {
        if e.is_timeout() {
            NotifyError::Timeout
        } else {
            NotifyError::Failed(e.without_url().to_string())
        }
    })?;
    let status = response.status();
    if status.is_success() {
        Ok(())
    } else {
        Err(NotifyError::Status(status.as_u16()))
    }
}
