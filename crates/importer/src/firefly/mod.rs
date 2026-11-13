//! A read-only client for the Firefly III API.
//!
//! The owner's requirement: Saldo only *reads* from Firefly. This client
//! therefore has no method that sends anything but `GET`.

pub mod models;
pub mod planning_models;

use std::fmt;
use std::time::Duration;

use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use thiserror::Error;

use models::{About, AboutResponse, Page, Resource};

/// Items per page; Firefly accepts much larger pages, this keeps responses small.
pub const PAGE_SIZE: u32 = 500;
const MAX_RETRIES: u32 = 4;
const DEFAULT_BACKOFF: Duration = Duration::from_millis(500);
const TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Error)]
pub enum FireflyError {
    #[error("invalid Firefly URL {0:?}")]
    InvalidUrl(String),
    /// Network or TLS failure (the message never contains the token).
    #[error("request to {path} failed: {message}")]
    Request { path: String, message: String },
    #[error("{path} answered HTTP {status}")]
    Status { path: String, status: u16 },
    #[error("{path} returned unexpected JSON: {message}")]
    Decode { path: String, message: String },
    #[error("{path}: expected {expected} items but received {received} distinct ones")]
    Incomplete {
        path: String,
        expected: u64,
        received: u64,
    },
}

/// The token, kept out of `Debug` output and therefore out of logs.
struct Token(String);

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

#[derive(Debug)]
pub struct FireflyClient {
    http: reqwest::Client,
    /// `https://firefly.example.com/api`, without a trailing slash.
    api: String,
    token: Token,
    backoff: Duration,
}

impl FireflyClient {
    /// `url` is the Firefly address as typed in a browser.
    pub fn new(url: &str, token: String) -> Result<Self, FireflyError> {
        let base = url.trim_end_matches('/');
        if !(base.starts_with("https://") || base.starts_with("http://")) {
            return Err(FireflyError::InvalidUrl(url.to_string()));
        }
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .user_agent(concat!("saldo-import/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| FireflyError::Request {
                path: String::new(),
                message: e.to_string(),
            })?;
        Ok(Self {
            http,
            api: format!("{base}/api"),
            token: Token(token),
            backoff: DEFAULT_BACKOFF,
        })
    }

    /// Shorter retry waits, for tests.
    pub fn with_backoff(self, backoff: Duration) -> Self {
        Self { backoff, ..self }
    }

    pub async fn about(&self) -> Result<About, FireflyError> {
        Ok(self.get_json::<AboutResponse>("/v1/about", &[]).await?.data)
    }

    /// Every item of a list endpoint, following `meta.pagination` page by
    /// page, without duplicates.
    ///
    /// `T: DeserializeOwned` is a *trait bound*: `list` works for any item
    /// type that serde can build from JSON, like a Go generic function with a
    /// constraint.
    pub async fn list<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<Vec<Resource<T>>, FireflyError> {
        let (items, _) = self.list_with_total(path, query).await?;
        let mut seen = std::collections::HashSet::new();
        Ok(items
            .into_iter()
            .filter(|item| seen.insert(item.id.clone()))
            .collect())
    }

    /// Every item of every page as received (an object may appear twice),
    /// plus the `meta.pagination.total` Firefly announced. Transactions are
    /// paged by split, so a group can arrive in two parts on two pages.
    pub async fn list_with_total<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<(Vec<Resource<T>>, u64), FireflyError> {
        let page_size = PAGE_SIZE.to_string();
        let mut items: Vec<Resource<T>> = Vec::new();
        let mut page = 1u32;
        loop {
            let page_text = page.to_string();
            let mut params = vec![("limit", page_size.as_str()), ("page", page_text.as_str())];
            params.extend_from_slice(query);
            let response: Page<T> = self.get_json(path, &params).await?;
            items.extend(response.data);
            if page >= response.meta.pagination.total_pages {
                return Ok((items, response.meta.pagination.total));
            }
            page += 1;
        }
    }

    /// The raw bytes of a download, for attachments.
    pub async fn get_bytes(&self, path: &str) -> Result<Vec<u8>, FireflyError> {
        let response = self.send(path, &[]).await?;
        let bytes = response.bytes().await.map_err(|e| request_error(path, e))?;
        Ok(bytes.to_vec())
    }

    async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, FireflyError> {
        let response = self.send(path, query).await?;
        let body = response.bytes().await.map_err(|e| request_error(path, e))?;
        serde_json::from_slice(&body).map_err(|e| FireflyError::Decode {
            path: path.to_string(),
            message: e.to_string(),
        })
    }

    /// One `GET`, retried with growing waits on 429 and 5xx answers.
    async fn send(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<reqwest::Response, FireflyError> {
        let url = format!("{}{path}", self.api);
        let mut attempt = 0;
        loop {
            let result = self
                .http
                .get(&url)
                .query(query)
                .bearer_auth(&self.token.0)
                .header(reqwest::header::ACCEPT, "application/vnd.api+json")
                .send()
                .await;
            let retry = match &result {
                Ok(response) => is_retryable(response.status()),
                Err(error) => error.is_timeout() || error.is_connect(),
            };
            if retry && attempt < MAX_RETRIES {
                attempt += 1;
                tokio::time::sleep(self.backoff * 2u32.pow(attempt - 1)).await;
                continue;
            }
            let response = result.map_err(|e| request_error(path, e))?;
            if !response.status().is_success() {
                return Err(FireflyError::Status {
                    path: path.to_string(),
                    status: response.status().as_u16(),
                });
            }
            return Ok(response);
        }
    }
}

fn is_retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// reqwest errors name the URL (which never holds the token); strip the
/// query string anyway to keep messages short.
fn request_error(path: &str, error: reqwest::Error) -> FireflyError {
    FireflyError::Request {
        path: path.to_string(),
        message: error.without_url().to_string(),
    }
}

/// Passes over a list before giving up on an incomplete result.
pub const MAX_PASSES: u32 = 3;

/// Checks that a list returned as many distinct items as Firefly announced.
pub fn check_complete(path: &str, expected: u64, received: usize) -> Result<(), FireflyError> {
    let received = received as u64;
    if received < expected {
        return Err(FireflyError::Incomplete {
            path: path.to_string(),
            expected,
            received,
        });
    }
    Ok(())
}
