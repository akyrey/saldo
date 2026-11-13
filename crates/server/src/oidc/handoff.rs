//! The desktop app's half of "Log in with Authentik". The login runs in the
//! system browser; the server then keeps the new session under a one-time
//! code and sends the browser to the app's loopback port with that code. The
//! app trades code plus its secret `verifier` for the token, so the token
//! never appears in a URL and a program that sees the code cannot use it
//! (RFC 8252, with PKCE's S256 check between app and server).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64ct::{Base64UrlUnpadded, Encoding};
use saldo_app::identity::LoginResult;
use sha2::{Digest, Sha256};

/// The app has this long to collect its token.
const MAX_AGE: Duration = Duration::from_secs(120);
/// Random bytes in a one-time code.
const CODE_BYTES: usize = 32;
/// Length of a base64url SHA-256 challenge.
const CHALLENGE_LEN: usize = 43;
/// Ports below this are privileged; the app picks a random high one.
const MIN_PORT: u16 = 1024;

/// `desktop_port` and `desktop_challenge` were not both present and valid.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("desktop_port (1024 or higher) and a base64url SHA-256 desktop_challenge go together")]
pub struct BadDesktopRequest;

/// Where the desktop app waits, and the challenge of its verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopReturn {
    pub port: u16,
    pub challenge: String,
}

impl DesktopReturn {
    /// Both parameters or neither; `Err` when they are present but unusable.
    pub fn parse(
        port: Option<u16>,
        challenge: Option<&str>,
    ) -> Result<Option<Self>, BadDesktopRequest> {
        match (port, challenge) {
            (None, None) => Ok(None),
            (Some(port), Some(challenge)) if port >= MIN_PORT && is_challenge(challenge) => {
                Ok(Some(Self {
                    port,
                    challenge: challenge.to_string(),
                }))
            }
            _ => Err(BadDesktopRequest),
        }
    }

    /// The app's loopback address with one query parameter.
    pub fn url(&self, name: &str, value: &str) -> String {
        format!("http://127.0.0.1:{}/callback?{name}={value}", self.port)
    }
}

fn is_challenge(text: &str) -> bool {
    text.len() == CHALLENGE_LEN
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

struct Handoff {
    login: LoginResult,
    challenge: String,
    created: Instant,
}

/// Sessions waiting for the desktop app, by one-time code.
#[derive(Default)]
pub struct DesktopHandoffs(Mutex<HashMap<String, Handoff>>);

impl DesktopHandoffs {
    /// Keeps `login` for the app and returns the code to send it.
    pub fn insert(&self, login: LoginResult, challenge: String) -> String {
        let mut bytes = [0u8; CODE_BYTES];
        rand::fill(&mut bytes);
        let code = Base64UrlUnpadded::encode_string(&bytes);
        let mut map = self.0.lock().unwrap_or_else(|p| p.into_inner());
        map.retain(|_, h| h.created.elapsed() < MAX_AGE);
        let handoff = Handoff {
            login,
            challenge,
            created: Instant::now(),
        };
        map.insert(code.clone(), handoff);
        code
    }

    /// The login for `code` if `verifier` matches its challenge. The code is
    /// spent either way, so a wrong guess cannot be followed by another.
    pub fn redeem(&self, code: &str, verifier: &str) -> Option<LoginResult> {
        let mut map = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let handoff = map.remove(code)?;
        let challenge = Base64UrlUnpadded::encode_string(&Sha256::digest(verifier.as_bytes()));
        let fresh = handoff.created.elapsed() < MAX_AGE;
        (fresh && challenge == handoff.challenge).then_some(handoff.login)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_parameters_come_together_and_well_formed() {
        let challenge = "a".repeat(CHALLENGE_LEN);
        assert_eq!(DesktopReturn::parse(None, None), Ok(None));
        assert!(
            DesktopReturn::parse(Some(53682), Some(&challenge))
                .unwrap()
                .is_some()
        );
        assert!(DesktopReturn::parse(Some(80), Some(&challenge)).is_err());
        assert!(DesktopReturn::parse(Some(53682), None).is_err());
        assert!(DesktopReturn::parse(Some(53682), Some("short")).is_err());
        assert!(DesktopReturn::parse(Some(53682), Some(&"+".repeat(CHALLENGE_LEN))).is_err());
    }
}
