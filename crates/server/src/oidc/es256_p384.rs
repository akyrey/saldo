//! Authentik's ECDSA quirk. Its default certificate is an ECDSA key on curve
//! P-384, yet it labels the ID tokens it signs `ES256` (an algorithm defined
//! for P-256) and signs over a SHA-256 hash. Standard libraries rightly refuse
//! that pair, so a login would always fail.
//!
//! This module checks exactly that pair by hand: a real ECDSA signature
//! check, on P-384, over SHA-256, with the provider's own published key
//! (found by key ID). Only a token labelled `ES256` whose key is P-384 takes
//! this path; every other token is checked by `openidconnect` as usual.

use base64ct::{Base64UrlUnpadded, Encoding};
use p384::ecdsa::signature::hazmat::PrehashVerifier;
use p384::ecdsa::{Signature, VerifyingKey};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// What this module decided about one ID token.
#[derive(Debug, PartialEq, Eq)]
pub enum Check {
    /// Not Authentik's pair: verify the standard way.
    NotApplicable,
    /// The signature is valid; the other claims still need checking.
    Valid,
    /// Authentik's pair, but the signature does not verify.
    Invalid(String),
}

#[derive(Deserialize)]
struct Header {
    alg: String,
    kid: Option<String>,
}

/// Checks `id_token` (the compact JWT) against the provider's `jwks`
/// (its JSON Web Key Set, as JSON).
pub fn check(id_token: &str, jwks: &Value) -> Check {
    let mut parts = id_token.split('.');
    let (Some(header_b64), Some(payload), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Check::NotApplicable;
    };
    let Some(header) = decode_json::<Header>(header_b64) else {
        return Check::NotApplicable;
    };
    if header.alg != "ES256" {
        return Check::NotApplicable;
    }
    let Some(key) = p384_key(jwks, header.kid.as_deref()) else {
        return Check::NotApplicable;
    };
    // The signature covers the first two segments exactly as received.
    match verify(&key, &format!("{header_b64}.{payload}"), signature) {
        Ok(()) => Check::Valid,
        Err(why) => Check::Invalid(why),
    }
}

fn decode_json<T: for<'de> Deserialize<'de>>(segment: &str) -> Option<T> {
    let bytes = Base64UrlUnpadded::decode_vec(segment).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// The P-384 key with this key ID (or the only P-384 key when the token
/// names none), as an uncompressed point `04 || x || y`.
fn p384_key(jwks: &Value, kid: Option<&str>) -> Option<VerifyingKey> {
    let keys = jwks.get("keys")?.as_array()?;
    let candidates: Vec<&Value> = keys
        .iter()
        .filter(|k| k["kty"] == "EC" && k["crv"] == "P-384")
        .filter(|k| kid.is_none_or(|kid| k["kid"] == kid))
        .collect();
    let [key] = candidates.as_slice() else {
        return None;
    };
    let x = Base64UrlUnpadded::decode_vec(key["x"].as_str()?).ok()?;
    let y = Base64UrlUnpadded::decode_vec(key["y"].as_str()?).ok()?;
    let point = [&[0x04][..], &x, &y].concat();
    VerifyingKey::from_sec1_bytes(&point).ok()
}

fn verify(key: &VerifyingKey, signed: &str, signature: &str) -> Result<(), String> {
    let bytes = Base64UrlUnpadded::decode_vec(signature).map_err(|e| e.to_string())?;
    let signature = Signature::from_slice(&bytes).map_err(|e| e.to_string())?;
    let digest = Sha256::digest(signed.as_bytes());
    key.verify_prehash(&digest, &signature)
        .map_err(|_| "the ES256 (P-384) signature does not match".to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use p384::ecdsa::SigningKey;
    use p384::ecdsa::signature::hazmat::PrehashSigner;
    use serde_json::json;

    use super::*;

    fn b64(bytes: &[u8]) -> String {
        Base64UrlUnpadded::encode_string(bytes)
    }

    /// A fixed test key, so no random numbers are needed.
    fn signing_key() -> SigningKey {
        SigningKey::from_slice(&[7u8; 48]).unwrap()
    }

    fn jwks(kid: &str) -> Value {
        let point = signing_key().verifying_key().to_sec1_bytes();
        // Uncompressed point: 0x04, then 48 bytes of x and 48 of y.
        json!({ "keys": [{
            "kty": "EC", "crv": "P-384", "alg": "ES256", "use": "sig", "kid": kid,
            "x": b64(&point[1..49]), "y": b64(&point[49..97]),
        }]})
    }

    /// Signs like Authentik: header says ES256, P-384 key, SHA-256 hash.
    fn token(alg: &str) -> String {
        let header = b64(json!({ "alg": alg, "kid": "k1", "typ": "JWT" })
            .to_string()
            .as_bytes());
        let payload = b64(br#"{"sub":"ak-1"}"#);
        let signed = format!("{header}.{payload}");
        let signature: Signature = signing_key()
            .sign_prehash(&Sha256::digest(signed.as_bytes()))
            .unwrap();
        format!("{signed}.{}", b64(&signature.to_bytes()))
    }

    #[test]
    fn accepts_a_genuine_authentik_signature() {
        assert_eq!(check(&token("ES256"), &jwks("k1")), Check::Valid);
    }

    #[test]
    fn refuses_a_tampered_token() {
        let original = token("ES256");
        let mut parts: Vec<&str> = original.split('.').collect();
        let other = b64(br#"{"sub":"someone-else"}"#);
        parts[1] = &other;
        assert!(matches!(
            check(&parts.join("."), &jwks("k1")),
            Check::Invalid(_)
        ));
    }

    #[test]
    fn leaves_everything_else_to_the_standard_check() {
        assert_eq!(check(&token("RS256"), &jwks("k1")), Check::NotApplicable);
        // A key ID the provider does not publish.
        assert_eq!(check(&token("ES256"), &jwks("other")), Check::NotApplicable);
        assert_eq!(check("not-a-jwt", &jwks("k1")), Check::NotApplicable);
        assert_eq!(
            check(&token("ES256"), &json!({ "keys": [] })),
            Check::NotApplicable
        );
    }
}
