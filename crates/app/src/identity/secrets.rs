//! Password hashing (Argon2id) and random session tokens (stored as SHA-256).

use std::sync::LazyLock;

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use sha2::{Digest, Sha256};

use crate::error::AppError;

/// Bytes of randomness in a token: 256 bits.
const TOKEN_BYTES: usize = 32;

/// Hashes a password with Argon2id and a random salt (a PHC string).
///
/// Hashing takes tens of milliseconds of pure CPU. `spawn_blocking` moves it
/// to a thread meant for blocking work so it does not stall the async
/// workers, which would delay every other request. The `move` closure takes
/// ownership of `password`, because the other thread may outlive this call.
pub(crate) async fn hash_password(password: String) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|hash| hash.to_string())
            .map_err(|e| AppError::Internal(format!("password hashing failed: {e}")))
    })
    .await
    .map_err(|e| AppError::Internal(format!("hashing task failed: {e}")))?
}

/// A real hash of a throwaway password. Checking a login for an unknown
/// email against it takes as long as a real check, so response times do
/// not reveal which emails exist.
static DUMMY_HASH: LazyLock<Option<String>> = LazyLock::new(|| {
    Argon2::default()
        .hash_password(b"saldo-dummy-password")
        .ok()
        .map(|hash| hash.to_string())
});

/// Checks `password` against a stored hash; `None` runs a dummy check and
/// returns false.
pub(crate) async fn verify_password(
    password: String,
    stored: Option<String>,
) -> Result<bool, AppError> {
    tokio::task::spawn_blocking(move || {
        let Some(hash) = stored.or_else(|| DUMMY_HASH.clone()) else {
            return false;
        };
        let matches = PasswordHash::new(&hash)
            .map(|parsed| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &parsed)
                    .is_ok()
            })
            .unwrap_or(false);
        matches && hash.as_str() != DUMMY_HASH.as_deref().unwrap_or("")
    })
    .await
    .map_err(|e| AppError::Internal(format!("verification task failed: {e}")))
}

/// A new random token (64 hex characters) to hand to the client once.
pub(crate) fn new_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    rand::fill(&mut bytes);
    to_hex(&bytes)
}

/// What the database stores instead of the token itself.
pub(crate) fn hash_token(token: &str) -> String {
    to_hex(&Sha256::digest(token.as_bytes()))
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hashed_passwords_verify_only_with_the_right_password() {
        let hash = hash_password("correct horse".to_string()).await.unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(
            verify_password("correct horse".to_string(), Some(hash.clone()))
                .await
                .unwrap()
        );
        assert!(
            !verify_password("wrong".to_string(), Some(hash))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn unknown_user_and_garbage_hashes_never_verify() {
        assert!(
            !verify_password("saldo-dummy-password".to_string(), None)
                .await
                .unwrap()
        );
        assert!(
            !verify_password("x".to_string(), Some("not a hash".to_string()))
                .await
                .unwrap()
        );
    }

    #[test]
    fn tokens_are_random_and_hashed_to_hex() {
        let token = new_token();
        assert_eq!(token.len(), 64);
        assert_ne!(token, new_token());
        assert_eq!(
            hash_token("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
