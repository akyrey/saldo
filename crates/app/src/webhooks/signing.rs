//! Webhook signatures (ARCHITECTURE.md 8.2): `X-Saldo-Signature:
//! t=<unix seconds>,v1=<hex>` where `v1 = HMAC-SHA256(secret, "<t>." + body)`.
//!
//! Pure functions: no clock, no I/O. The caller passes the time.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use thiserror::Error;

/// HMAC with SHA-256. A type alias is a short name for a longer type, like
/// `type Handler func(...)` in Go.
type HmacSha256 = Hmac<Sha256>;

/// How old a signature may be before receivers (and [`verify`]) reject it.
pub const DEFAULT_TOLERANCE_SECONDS: i64 = 5 * 60;

/// Why a signature was rejected.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SignatureError {
    #[error("signature header is malformed")]
    Malformed,
    #[error("signature timestamp is outside the tolerance")]
    Expired,
    #[error("signature does not match")]
    Mismatch,
}

/// The `v1` value: lowercase hex of `HMAC-SHA256(secret, "<timestamp>." + body)`.
///
/// `body: &[u8]` borrows the bytes: the function reads them while it runs and
/// keeps nothing, so the compiler needs no lifetime annotation here. (PHP
/// copies strings on write; Go passes a slice header; Rust's borrow is the
/// slice header plus a compile-time promise that nobody frees the bytes
/// while we read them.)
pub fn sign(secret: &str, timestamp: i64, body: &[u8]) -> String {
    to_hex(&mac(secret, timestamp, body).finalize().into_bytes())
}

/// The full header value: `t=<timestamp>,v1=<hex>`.
pub fn signature_header(secret: &str, timestamp: i64, body: &[u8]) -> String {
    format!("t={timestamp},v1={}", sign(secret, timestamp, body))
}

/// Checks a header value as a receiver would: the timestamp must be within
/// `tolerance_seconds` of `now`, and one `v1` must match in constant time.
pub fn verify(
    secret: &str,
    header: &str,
    body: &[u8],
    now: i64,
    tolerance_seconds: i64,
) -> Result<(), SignatureError> {
    let (timestamp, signatures) = parse_header(header)?;
    if (now - timestamp).abs() > tolerance_seconds {
        return Err(SignatureError::Expired);
    }
    let matches = signatures.iter().any(|candidate| {
        // `verify_slice` compares in constant time, so the response time does
        // not reveal how many leading bytes were right.
        mac(secret, timestamp, body).verify_slice(candidate).is_ok()
    });
    if matches {
        Ok(())
    } else {
        Err(SignatureError::Mismatch)
    }
}

fn mac(secret: &str, timestamp: i64, body: &[u8]) -> HmacSha256 {
    // HMAC accepts keys of any length, so this never fails; the fallback
    // keeps `expect` out of production code.
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .unwrap_or_else(|_| HmacSha256::new(&Default::default()));
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    mac
}

/// `t=123,v1=abcd,v1=ef01` → `(123, [bytes, bytes])`. Unknown keys are
/// ignored, so a later `v2` does not break old receivers.
fn parse_header(header: &str) -> Result<(i64, Vec<Vec<u8>>), SignatureError> {
    let mut timestamp = None;
    let mut signatures = Vec::new();
    for part in header.split(',') {
        let (key, value) = part
            .trim()
            .split_once('=')
            .ok_or(SignatureError::Malformed)?;
        match key {
            "t" => timestamp = Some(value.parse().map_err(|_| SignatureError::Malformed)?),
            "v1" => signatures.push(from_hex(value).ok_or(SignatureError::Malformed)?),
            _ => {}
        }
    }
    match timestamp {
        Some(t) if !signatures.is_empty() => Ok((t, signatures)),
        _ => Err(SignatureError::Malformed),
    }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "whsec_test";
    const T: i64 = 1_794_428_460;
    const BODY: &[u8] = br#"{"id":"1"}"#;
    // Computed independently with Python's `hmac` module.
    const EXPECTED: &str = "7a64ecf7bcdeeda8dd77aa39fddd013ac2b879b24dfc23c8a772f83070ff68c3";

    #[test]
    fn signs_with_a_fixed_vector() {
        assert_eq!(sign(SECRET, T, BODY), EXPECTED);
        assert_eq!(
            sign("key", 0, b""),
            "85841b4efc3cd7776c3c8f9b7cca9e281c550e5d19889d78e9e669c6337f000d"
        );
        assert_eq!(
            signature_header(SECRET, T, BODY),
            format!("t={T},v1={EXPECTED}")
        );
    }

    #[test]
    fn verifies_its_own_signatures() {
        let header = signature_header(SECRET, T, BODY);
        assert_eq!(verify(SECRET, &header, BODY, T + 10, 300), Ok(()));
        // Any v1 may match (for example during a secret rotation).
        let two = format!("t={T},v1={},v1={EXPECTED}", "00".repeat(32));
        assert_eq!(verify(SECRET, &two, BODY, T, 300), Ok(()));
    }

    #[test]
    fn rejects_wrong_secret_body_or_timestamp() {
        let header = signature_header(SECRET, T, BODY);
        assert_eq!(
            verify("other", &header, BODY, T, 300),
            Err(SignatureError::Mismatch)
        );
        assert_eq!(
            verify(SECRET, &header, b"{}", T, 300),
            Err(SignatureError::Mismatch)
        );
        let moved = header.replace(&T.to_string(), &(T + 1).to_string());
        assert_eq!(
            verify(SECRET, &moved, BODY, T, 300),
            Err(SignatureError::Mismatch)
        );
    }

    #[test]
    fn rejects_old_and_future_timestamps() {
        let header = signature_header(SECRET, T, BODY);
        assert_eq!(
            verify(SECRET, &header, BODY, T + 301, 300),
            Err(SignatureError::Expired)
        );
        assert_eq!(
            verify(SECRET, &header, BODY, T - 301, 300),
            Err(SignatureError::Expired)
        );
    }

    #[test]
    fn rejects_malformed_headers() {
        for header in [
            "",
            "t=1",
            "v1=00",
            "t=x,v1=00",
            "t=1,v1=0",
            "t=1,v1=zz",
            "garbage",
        ] {
            assert_eq!(
                verify(SECRET, header, BODY, 1, 300),
                Err(SignatureError::Malformed),
                "{header}"
            );
        }
        // Unknown keys are ignored.
        let header = format!("t={T},v0=xx,v1={EXPECTED}");
        assert_eq!(verify(SECRET, &header, BODY, T, 300), Ok(()));
    }
}
