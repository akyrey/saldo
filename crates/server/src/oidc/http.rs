//! The HTTP client `openidconnect` uses: our `reqwest` (rustls), with
//! redirects off so a provider response can never send Saldo elsewhere.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use openidconnect::{AsyncHttpClient, HttpRequest, HttpResponse};

/// Requests to the provider give up after this long.
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
#[error("identity provider request failed: {0}")]
pub struct HttpError(String);

pub struct OidcHttp(reqwest::Client);

impl OidcHttp {
    pub fn new() -> Result<Self, HttpError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(TIMEOUT)
            .build()
            .map_err(|e| HttpError(e.to_string()))?;
        Ok(Self(client))
    }
}

// `openidconnect` asks for "something that sends an HTTP request
// asynchronously" through this trait, like a Go interface it defines and we
// implement with our own client.
impl<'c> AsyncHttpClient<'c> for OidcHttp {
    type Error = HttpError;
    type Future = Pin<Box<dyn Future<Output = Result<HttpResponse, HttpError>> + Send + 'c>>;

    fn call(&'c self, request: HttpRequest) -> Self::Future {
        Box::pin(async move {
            let request =
                reqwest::Request::try_from(request).map_err(|e| HttpError(e.to_string()))?;
            let response = self
                .0
                .execute(request)
                .await
                .map_err(|e| HttpError(e.without_url().to_string()))?;
            let mut builder = openidconnect::http::Response::builder().status(response.status());
            for (name, value) in response.headers() {
                builder = builder.header(name, value);
            }
            let body = response
                .bytes()
                .await
                .map_err(|e| HttpError(e.to_string()))?;
            builder
                .body(body.to_vec())
                .map_err(|e| HttpError(e.to_string()))
        })
    }
}
