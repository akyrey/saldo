//! Currencies.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::CurrencyError;

/// The largest number of decimal places a currency may have.
pub const MAX_DECIMAL_PLACES: u8 = 8;

/// An ISO 4217 style code: exactly three uppercase ASCII letters, such as `EUR`.
///
/// Stored as three bytes, so it is `Copy`: passing it around copies it like an
/// integer, with no allocation and no ownership to think about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CurrencyCode([u8; 3]);

impl CurrencyCode {
    /// Checks the input and builds a code. `Self` means "this type".
    pub fn new(input: &str) -> Result<Self, CurrencyError> {
        // `match` must cover every case or the code does not compile; here the
        // only accepted shape is a 3-byte array of uppercase ASCII letters.
        match input.as_bytes() {
            &[a, b, c] if [a, b, c].iter().all(u8::is_ascii_uppercase) => Ok(Self([a, b, c])),
            _ => Err(CurrencyError::InvalidCode {
                input: input.to_string(),
            }),
        }
    }
}

impl fmt::Display for CurrencyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{}", char::from(byte))?;
        }
        Ok(())
    }
}

impl FromStr for CurrencyCode {
    type Err = CurrencyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

// These two conversions are what `#[serde(try_from, into)]` above uses.
impl TryFrom<String> for CurrencyCode {
    type Error = CurrencyError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<CurrencyCode> for String {
    fn from(code: CurrencyCode) -> Self {
        code.to_string()
    }
}

/// A currency and how many decimal places its amounts have (EUR 2, JPY 0).
///
/// The fields are private so that a `Currency` with 12 decimal places cannot
/// exist: the only ways in are [`Currency::new`] and deserialising, and both
/// check the value. (PHP: a final class with a private constructor and a
/// validating static factory.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "CurrencyFields")]
pub struct Currency {
    code: CurrencyCode,
    decimal_places: u8,
}

impl Currency {
    pub fn new(code: CurrencyCode, decimal_places: u8) -> Result<Self, CurrencyError> {
        if decimal_places > MAX_DECIMAL_PLACES {
            return Err(CurrencyError::InvalidDecimalPlaces {
                places: decimal_places,
            });
        }
        Ok(Self {
            code,
            decimal_places,
        })
    }

    pub fn code(&self) -> CurrencyCode {
        self.code
    }

    pub fn decimal_places(&self) -> u8 {
        self.decimal_places
    }
}

/// The unchecked JSON shape of a [`Currency`], used only while deserialising.
#[derive(Deserialize)]
struct CurrencyFields {
    code: CurrencyCode,
    decimal_places: u8,
}

impl TryFrom<CurrencyFields> for Currency {
    type Error = CurrencyError;

    fn try_from(fields: CurrencyFields) -> Result<Self, Self::Error> {
        Self::new(fields.code, fields.decimal_places)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_three_uppercase_letters() {
        let code = CurrencyCode::new("EUR").unwrap();
        assert_eq!(code.to_string(), "EUR");
        assert_eq!("JPY".parse::<CurrencyCode>().unwrap().to_string(), "JPY");
    }

    #[test]
    fn rejects_invalid_codes() {
        for input in ["", "EU", "EURO", "eur", "Eur", "E1R", "€UR", " EU"] {
            assert_eq!(
                CurrencyCode::new(input),
                Err(CurrencyError::InvalidCode {
                    input: input.to_string()
                }),
                "input {input:?}"
            );
        }
    }

    #[test]
    fn code_serialises_as_a_string() {
        let code = CurrencyCode::new("USD").unwrap();
        assert_eq!(serde_json::to_string(&code).unwrap(), "\"USD\"");
        assert_eq!(
            serde_json::from_str::<CurrencyCode>("\"USD\"").unwrap(),
            code
        );
        assert!(serde_json::from_str::<CurrencyCode>("\"usd\"").is_err());
    }

    #[test]
    fn currency_accepts_zero_to_eight_places() {
        let eur = CurrencyCode::new("EUR").unwrap();
        for places in 0..=MAX_DECIMAL_PLACES {
            let currency = Currency::new(eur, places).unwrap();
            assert_eq!(currency.code(), eur);
            assert_eq!(currency.decimal_places(), places);
        }
    }

    #[test]
    fn currency_rejects_more_than_eight_places() {
        let eur = CurrencyCode::new("EUR").unwrap();
        assert_eq!(
            Currency::new(eur, 9),
            Err(CurrencyError::InvalidDecimalPlaces { places: 9 })
        );
    }

    #[test]
    fn currency_round_trips_through_json_and_is_checked() {
        let eur = Currency::new(CurrencyCode::new("EUR").unwrap(), 2).unwrap();
        let json = serde_json::to_string(&eur).unwrap();
        assert_eq!(json, r#"{"code":"EUR","decimal_places":2}"#);
        assert_eq!(serde_json::from_str::<Currency>(&json).unwrap(), eur);
        let too_many = r#"{"code":"EUR","decimal_places":12}"#;
        assert!(serde_json::from_str::<Currency>(too_many).is_err());
    }

    #[test]
    fn error_messages_are_readable() {
        let err = CurrencyCode::new("eu").unwrap_err();
        assert_eq!(
            err.to_string(),
            "currency code must be three uppercase ASCII letters, got \"eu\""
        );
    }
}
