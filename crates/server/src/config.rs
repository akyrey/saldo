//! Configuration from `SALDO_*` environment variables.

use std::fmt;
use std::net::SocketAddr;

const DEFAULT_DATABASE_URL: &str = "sqlite:///data/saldo.db";
const DEFAULT_BIND: &str = "0.0.0.0:8080";
const DEFAULT_PUBLIC_URL: &str = "http://localhost:8080";
const DEFAULT_LOG: &str = "info";
const DEFAULT_DATA_DIR: &str = "/data";

/// Server settings. Invalid values stop the server at startup with a message
/// naming the variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub database_url: String,
    /// Folder for attachment files (`<data dir>/attachments/`).
    pub data_dir: std::path::PathBuf,
    pub bind: SocketAddr,
    /// Where users reach Saldo, for example `https://money.example.com`.
    pub public_url: String,
    /// Scheme, host and port of `public_url`; requests authenticated by
    /// cookie must send this as their `Origin`.
    pub public_origin: String,
    /// The `tracing` filter, for example `info` or `saldo=debug,info`.
    pub log: String,
    /// Mark the session cookie `Secure` (when `public_url` is https).
    pub cookie_secure: bool,
    /// SMTP and VAPID settings for notifications (`SALDO_SMTP_*`, `SALDO_VAPID_*`).
    pub notify: crate::notify::config::NotifyConfig,
    /// Origins allowed by CORS (`SALDO_ALLOWED_ORIGINS`, comma-separated),
    /// for example the desktop app's `tauri://localhost`. Empty: CORS off.
    pub allowed_origins: Vec<String>,
}

/// A configuration problem: which variable, and what is wrong with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub variable: &'static str,
    pub message: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.variable, self.message)
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Reads settings through `lookup`, so tests need no real environment.
    /// `impl Fn(&str) -> Option<String>` accepts any function or closure with
    /// that shape: Go's `func(string) (string, bool)` parameter.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let get = |name: &str, default: &str| {
            lookup(name)
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| default.to_string())
        };

        let database_url = get("SALDO_DATABASE_URL", DEFAULT_DATABASE_URL);
        if !database_url.starts_with("sqlite:") {
            return Err(error("SALDO_DATABASE_URL", "must start with sqlite:"));
        }
        let bind = get("SALDO_BIND", DEFAULT_BIND)
            .parse()
            .map_err(|_| error("SALDO_BIND", "must be an address like 0.0.0.0:8080"))?;
        let public_url = get("SALDO_PUBLIC_URL", DEFAULT_PUBLIC_URL)
            .trim_end_matches('/')
            .to_string();
        let public_origin = origin_of(&public_url)
            .ok_or_else(|| error("SALDO_PUBLIC_URL", "must be an http:// or https:// URL"))?;
        let notify = crate::notify::config::from_lookup(&lookup, &public_url)?;
        let allowed_origins = allowed_origins(&get("SALDO_ALLOWED_ORIGINS", ""))?;
        Ok(Self {
            notify,
            database_url,
            data_dir: get("SALDO_DATA_DIR", DEFAULT_DATA_DIR).into(),
            bind,
            cookie_secure: public_url.starts_with("https://"),
            public_origin,
            public_url,
            log: get("SALDO_LOG", DEFAULT_LOG),
            allowed_origins,
        })
    }
}

/// `tauri://localhost, http://tauri.localhost` → the two origins. Each must
/// be `scheme://host[:port]`, with no path, as browsers send `Origin`.
fn allowed_origins(text: &str) -> Result<Vec<String>, ConfigError> {
    let bad = || {
        error(
            "SALDO_ALLOWED_ORIGINS",
            "must be origins like tauri://localhost",
        )
    };
    text.split(',')
        .map(str::trim)
        .filter(|o| !o.is_empty())
        .map(|origin| {
            let (scheme, host) = origin.split_once("://").ok_or_else(bad)?;
            let valid_scheme =
                !scheme.is_empty() && scheme.chars().all(|c| c.is_ascii_alphanumeric());
            let valid_host = !host.is_empty() && !host.contains(['/', ' ', '?', '#']);
            if valid_scheme && valid_host {
                Ok(origin.to_string())
            } else {
                Err(bad())
            }
        })
        .collect()
}

fn error(variable: &'static str, message: &str) -> ConfigError {
    ConfigError {
        variable,
        message: message.to_string(),
    }
}

/// `https://example.com:8443/saldo` → `https://example.com:8443`.
fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let host = rest.split('/').next().filter(|h| !h.is_empty())?;
    Some(format!("{scheme}://{host}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn config(vars: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_lookup(|name| map.get(name).cloned())
    }

    #[test]
    fn defaults_apply_when_nothing_is_set() {
        let config = config(&[]).unwrap();
        assert_eq!(config.database_url, "sqlite:///data/saldo.db");
        assert_eq!(config.bind.to_string(), "0.0.0.0:8080");
        assert_eq!(config.public_origin, "http://localhost:8080");
        assert_eq!(config.log, "info");
        assert_eq!(config.data_dir, std::path::PathBuf::from("/data"));
        assert!(!config.cookie_secure);
    }

    #[test]
    fn reads_and_normalises_values() {
        let config = config(&[
            ("SALDO_PUBLIC_URL", "https://money.example.com/saldo/"),
            ("SALDO_BIND", "127.0.0.1:9000"),
            ("SALDO_DATABASE_URL", "sqlite://saldo.db"),
        ])
        .unwrap();
        assert_eq!(config.public_url, "https://money.example.com/saldo");
        assert_eq!(config.public_origin, "https://money.example.com");
        assert!(config.cookie_secure);
        assert_eq!(config.bind.port(), 9000);
    }

    #[test]
    fn errors_name_the_variable() {
        let cases = [
            ("SALDO_BIND", "nope"),
            ("SALDO_PUBLIC_URL", "ftp://x"),
            ("SALDO_PUBLIC_URL", "https://"),
            ("SALDO_DATABASE_URL", "postgres://x"),
        ];
        for (variable, value) in cases {
            let err = config(&[(variable, value)]).unwrap_err();
            assert_eq!(err.variable, variable);
            assert!(err.to_string().starts_with(variable));
        }
    }
}
