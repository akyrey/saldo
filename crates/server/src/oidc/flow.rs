//! The two halves of a login: `start` builds the provider URL, `finish`
//! turns the returned code into a verified identity.

use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
};
use saldo_app::identity::ExternalIdentity;

use super::config::OidcConfig;
use super::http::OidcHttp;
use super::pending::Pending;

/// Why a login through the provider failed. The code goes into the login
/// page's URL (`/login?error=...`); details only into the log.
#[derive(Debug, thiserror::Error)]
pub enum OidcError {
    #[error("the identity provider could not be reached or answered wrongly: {0}")]
    Provider(String),
    #[error("the login expired or did not start here")]
    UnknownState,
    #[error("the identity provider refused: {0}")]
    Refused(String),
    #[error("the ID token is invalid: {0}")]
    InvalidToken(String),
}

impl OidcError {
    /// The short code shown to the login page.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Provider(_) => "oidc_unavailable",
            Self::UnknownState => "oidc_expired",
            Self::Refused(_) => "oidc_refused",
            Self::InvalidToken(_) => "oidc_invalid",
        }
    }
}

fn provider<E: std::fmt::Display>(error: E) -> OidcError {
    OidcError::Provider(error.to_string())
}

/// The provider's settings, fetched from its discovery document each login
/// (logins are rare, and so a rotated signing key is always picked up).
async fn metadata(config: &OidcConfig, http: &OidcHttp) -> Result<CoreProviderMetadata, OidcError> {
    let issuer = IssuerUrl::new(config.issuer.clone()).map_err(provider)?;
    CoreProviderMetadata::discover_async(issuer, http)
        .await
        .map_err(provider)
}

/// The URL to send the browser to, the `state` that identifies this login,
/// and what the callback needs to finish it.
pub async fn start(
    config: &OidcConfig,
    callback_url: &str,
    redirect: String,
) -> Result<(String, String, Pending), OidcError> {
    let http = OidcHttp::new().map_err(provider)?;
    let client = CoreClient::from_provider_metadata(
        metadata(config, &http).await?,
        ClientId::new(config.client_id.clone()),
        Some(ClientSecret::new(config.client_secret.clone())),
    )
    .set_redirect_uri(RedirectUrl::new(callback_url.to_string()).map_err(provider)?);
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let (url, state, nonce) = client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .set_pkce_challenge(challenge)
        .url();
    let pending = Pending::new(verifier.secret().clone(), nonce.secret().clone(), redirect);
    Ok((url.to_string(), state.secret().clone(), pending))
}

/// Exchanges `code` for tokens and returns who logged in, after checking the
/// ID token's signature, issuer, audience, expiry and nonce.
pub async fn finish(
    config: &OidcConfig,
    callback_url: &str,
    pending: Pending,
    code: String,
) -> Result<ExternalIdentity, OidcError> {
    let http = OidcHttp::new().map_err(provider)?;
    let client = CoreClient::from_provider_metadata(
        metadata(config, &http).await?,
        ClientId::new(config.client_id.clone()),
        Some(ClientSecret::new(config.client_secret.clone())),
    )
    .set_redirect_uri(RedirectUrl::new(callback_url.to_string()).map_err(provider)?);
    let tokens = client
        .exchange_code(AuthorizationCode::new(code))
        .map_err(provider)?
        .set_pkce_verifier(PkceCodeVerifier::new(pending.pkce_verifier))
        .request_async(&http)
        .await
        .map_err(|e| OidcError::Refused(e.to_string()))?;
    let id_token = tokens
        .id_token()
        .ok_or_else(|| OidcError::InvalidToken("no ID token in the answer".into()))?;
    let claims = id_token
        .claims(&client.id_token_verifier(), &Nonce::new(pending.nonce))
        .map_err(|e| OidcError::InvalidToken(e.to_string()))?;
    Ok(ExternalIdentity {
        issuer: claims.issuer().to_string(),
        subject: claims.subject().to_string(),
        email: claims.email().map(|e| e.to_string()),
        email_verified: claims.email_verified().unwrap_or(false),
    })
}
