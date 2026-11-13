//! `SALDO_OIDC_*`: the identity provider. All three main variables or none.

use std::fmt;

use crate::config::ConfigError;

/// Shown on the login button when `SALDO_OIDC_NAME` is not set.
const DEFAULT_NAME: &str = "Authentik";

#[derive(Clone, PartialEq, Eq)]
pub struct OidcConfig {
    /// For Authentik: `https://auth.example.com/application/o/<slug>/`.
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    /// The provider's name on the login button.
    pub name: String,
}

// The secret never reaches a log line, even through `{:?}`.
impl fmt::Debug for OidcConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OidcConfig")
            .field("issuer", &self.issuer)
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .field("name", &self.name)
            .finish()
    }
}

fn error(variable: &'static str, message: &str) -> ConfigError {
    ConfigError {
        variable,
        message: message.to_string(),
    }
}

/// The provider, if `SALDO_OIDC_ISSUER` is set.
pub fn from_lookup(
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<Option<OidcConfig>, ConfigError> {
    let get = |name: &str| {
        lookup(name)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let (issuer, id, secret) = (
        get("SALDO_OIDC_ISSUER"),
        get("SALDO_OIDC_CLIENT_ID"),
        get("SALDO_OIDC_CLIENT_SECRET"),
    );
    let Some(issuer) = issuer else {
        if id.is_some() || secret.is_some() {
            return Err(error(
                "SALDO_OIDC_ISSUER",
                "is required with a client ID or secret",
            ));
        }
        return Ok(None);
    };
    if !(issuer.starts_with("https://") || issuer.starts_with("http://")) {
        return Err(error("SALDO_OIDC_ISSUER", "must be an http(s) URL"));
    }
    let client_id =
        id.ok_or_else(|| error("SALDO_OIDC_CLIENT_ID", "is required with an issuer"))?;
    let client_secret =
        secret.ok_or_else(|| error("SALDO_OIDC_CLIENT_SECRET", "is required with an issuer"))?;
    Ok(Some(OidcConfig {
        issuer,
        client_id,
        client_secret,
        name: get("SALDO_OIDC_NAME").unwrap_or_else(|| DEFAULT_NAME.to_string()),
    }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn lookup(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn reads_all_three_or_nothing() {
        assert_eq!(from_lookup(&lookup(&[])).unwrap(), None);
        let config = from_lookup(&lookup(&[
            (
                "SALDO_OIDC_ISSUER",
                "https://auth.example.com/application/o/saldo/",
            ),
            ("SALDO_OIDC_CLIENT_ID", "saldo"),
            ("SALDO_OIDC_CLIENT_SECRET", "s3cret"),
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(config.name, "Authentik");
        assert!(!format!("{config:?}").contains("s3cret"));

        let missing = from_lookup(&lookup(&[("SALDO_OIDC_CLIENT_ID", "saldo")])).unwrap_err();
        assert_eq!(missing.variable, "SALDO_OIDC_ISSUER");
        let no_secret = from_lookup(&lookup(&[
            ("SALDO_OIDC_ISSUER", "https://auth.example.com/"),
            ("SALDO_OIDC_CLIENT_ID", "saldo"),
        ]))
        .unwrap_err();
        assert_eq!(no_secret.variable, "SALDO_OIDC_CLIENT_SECRET");
        let bad = from_lookup(&lookup(&[
            ("SALDO_OIDC_ISSUER", "auth.example.com"),
            ("SALDO_OIDC_CLIENT_ID", "saldo"),
            ("SALDO_OIDC_CLIENT_SECRET", "x"),
        ]))
        .unwrap_err();
        assert_eq!(bad.variable, "SALDO_OIDC_ISSUER");
    }
}
