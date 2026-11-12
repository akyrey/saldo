//! Money: an exact amount in minor units (cents) plus its currency.

use serde::{Deserialize, Serialize};

use crate::currency::{Currency, CurrencyCode};
use crate::error::MoneyError;

/// An amount of money. `minor` counts the smallest unit, so EUR 123.45 is
/// `12345`. Never a float.
///
/// A struct plus an `impl` block is like a PHP final class with methods, or a Go
/// struct with methods; there is no inheritance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Money {
    minor: i64,
    currency: CurrencyCode,
}

impl Money {
    pub fn new(minor: i64, currency: CurrencyCode) -> Self {
        Self { minor, currency }
    }

    pub fn zero(currency: CurrencyCode) -> Self {
        Self::new(0, currency)
    }

    pub fn minor(&self) -> i64 {
        self.minor
    }

    pub fn currency(&self) -> CurrencyCode {
        self.currency
    }

    pub fn is_positive(&self) -> bool {
        self.minor > 0
    }

    /// The same amount with the opposite sign. `i64::MIN` has no positive
    /// counterpart, so it saturates to `i64::MAX` instead of overflowing.
    pub fn negated(&self) -> Self {
        Self::new(self.minor.saturating_neg(), self.currency)
    }

    /// Parses a plain decimal such as `"123.45"` or `"-3.10"` into minor units.
    ///
    /// Accepted: an optional leading `-`, digits, then optionally `.` and
    /// between one and `decimal_places` digits. Everything else (spaces, `+`,
    /// `,`, exponents) is rejected rather than guessed at.
    ///
    /// `&Currency` is a *borrow*: the function may read the currency but the
    /// caller keeps it. Close to passing a Go pointer you promise not to modify,
    /// except the compiler checks the promise.
    pub fn parse_decimal(input: &str, currency: &Currency) -> Result<Money, MoneyError> {
        let invalid = || MoneyError::InvalidFormat {
            input: input.to_string(),
        };
        let (negative, unsigned) = match input.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, input),
        };
        let (whole, fraction) = match unsigned.split_once('.') {
            Some((whole, fraction)) if is_digits(fraction) => (whole, fraction),
            Some(_) => return Err(invalid()),
            None => (unsigned, ""),
        };
        if !is_digits(whole) {
            return Err(invalid());
        }
        let places = currency.decimal_places();
        if fraction.len() > usize::from(places) {
            return Err(MoneyError::TooManyDecimalPlaces {
                input: input.to_string(),
                allowed: places,
            });
        }
        // `?` returns the error early: Go's `if err != nil { return err }`.
        let whole_minor = parse_digits(whole)?
            .checked_mul(ten_to_the(places))
            .ok_or(MoneyError::Overflow)?;
        let fraction_minor = pad_fraction(fraction, places)?;
        let minor = whole_minor
            .checked_add(fraction_minor)
            .ok_or(MoneyError::Overflow)?;
        let signed = if negative { -minor } else { minor };
        Ok(Money::new(signed, currency.code()))
    }

    /// Formats with exactly the currency's decimal places: `12345` → `"123.45"`.
    ///
    /// Only the decimal places are taken from `currency`; the caller passes the
    /// currency that matches this amount.
    pub fn to_decimal_string(&self, currency: &Currency) -> String {
        let places = currency.decimal_places();
        if places == 0 {
            return self.minor.to_string();
        }
        // `unsigned_abs` handles i64::MIN, whose absolute value does not fit i64.
        let scale = ten_to_the(places).unsigned_abs();
        let abs = self.minor.unsigned_abs();
        let sign = if self.minor < 0 { "-" } else { "" };
        let width = usize::from(places);
        format!("{sign}{}.{:0width$}", abs / scale, abs % scale)
    }

    /// Adds two amounts of the same currency. Errors on overflow or mismatch.
    pub fn checked_add(&self, other: Money) -> Result<Money, MoneyError> {
        self.ensure_same_currency(other)?;
        let minor = self
            .minor
            .checked_add(other.minor)
            .ok_or(MoneyError::Overflow)?;
        Ok(Money::new(minor, self.currency))
    }

    /// Subtracts `other`. Errors on overflow or mismatch.
    pub fn checked_sub(&self, other: Money) -> Result<Money, MoneyError> {
        self.ensure_same_currency(other)?;
        let minor = self
            .minor
            .checked_sub(other.minor)
            .ok_or(MoneyError::Overflow)?;
        Ok(Money::new(minor, self.currency))
    }

    fn ensure_same_currency(&self, other: Money) -> Result<(), MoneyError> {
        if self.currency == other.currency {
            Ok(())
        } else {
            Err(MoneyError::CurrencyMismatch {
                left: self.currency,
                right: other.currency,
            })
        }
    }
}

fn is_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// Parses a string already known to be digits; the only failure is overflow.
fn parse_digits(digits: &str) -> Result<i64, MoneyError> {
    digits.parse::<i64>().map_err(|_| MoneyError::Overflow)
}

/// Right-pads the fraction with zeros to the currency's places, so `"5"` with
/// 2 places is 50 minor units. `""` is 0.
fn pad_fraction(fraction: &str, places: u8) -> Result<i64, MoneyError> {
    if fraction.is_empty() {
        return Ok(0);
    }
    let width = usize::from(places);
    parse_digits(&format!("{fraction:0<width$}"))
}

/// 10 to the power `places`. `Currency` guarantees `places <= 8`, so this fits.
fn ten_to_the(places: u8) -> i64 {
    10_i64.pow(u32::from(places))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn currency(code: &str, places: u8) -> Currency {
        Currency::new(CurrencyCode::new(code).unwrap(), places).unwrap()
    }

    fn eur() -> Currency {
        currency("EUR", 2)
    }

    fn parse_eur(input: &str) -> Result<i64, MoneyError> {
        Money::parse_decimal(input, &eur()).map(|m| m.minor())
    }

    #[test]
    fn parses_decimals_into_minor_units() {
        assert_eq!(parse_eur("123.45"), Ok(12345));
        assert_eq!(parse_eur("0.5"), Ok(50));
        assert_eq!(parse_eur("7"), Ok(700));
        assert_eq!(parse_eur("-3.10"), Ok(-310));
        assert_eq!(parse_eur("0"), Ok(0));
        assert_eq!(parse_eur("0007.00"), Ok(700));
    }

    #[test]
    fn parsed_money_has_the_currency_code() {
        let money = Money::parse_decimal("1", &eur()).unwrap();
        assert_eq!(money.currency(), eur().code());
    }

    #[test]
    fn rejects_too_many_decimal_places() {
        assert_eq!(
            parse_eur("1.234"),
            Err(MoneyError::TooManyDecimalPlaces {
                input: "1.234".to_string(),
                allowed: 2
            })
        );
    }

    #[test]
    fn zero_decimal_currency() {
        let jpy = currency("JPY", 0);
        assert_eq!(Money::parse_decimal("1500", &jpy).unwrap().minor(), 1500);
        assert!(Money::parse_decimal("1500.5", &jpy).is_err());
        assert_eq!(Money::new(1500, jpy.code()).to_decimal_string(&jpy), "1500");
        assert_eq!(Money::new(-7, jpy.code()).to_decimal_string(&jpy), "-7");
    }

    #[test]
    fn rejects_malformed_input() {
        for input in [
            "", "abc", "1e3", "1,5", "-", ".5", "7.", "+1", " 1", "1 ", "1.2.3", "--1", "1.-5",
        ] {
            assert_eq!(
                parse_eur(input),
                Err(MoneyError::InvalidFormat {
                    input: input.to_string()
                }),
                "input {input:?}"
            );
        }
    }

    #[test]
    fn rejects_amounts_that_overflow() {
        assert_eq!(parse_eur("92233720368547758.08"), Err(MoneyError::Overflow));
        assert_eq!(parse_eur("99999999999999999999"), Err(MoneyError::Overflow));
        assert_eq!(parse_eur("92233720368547758.07"), Ok(i64::MAX));
    }

    #[test]
    fn formats_with_the_currency_places() {
        let eur = eur();
        let code = eur.code();
        assert_eq!(Money::new(12345, code).to_decimal_string(&eur), "123.45");
        assert_eq!(Money::new(5, code).to_decimal_string(&eur), "0.05");
        assert_eq!(Money::new(-310, code).to_decimal_string(&eur), "-3.10");
        assert_eq!(Money::new(-5, code).to_decimal_string(&eur), "-0.05");
        assert_eq!(Money::zero(code).to_decimal_string(&eur), "0.00");
        assert_eq!(
            Money::new(i64::MIN, code).to_decimal_string(&eur),
            "-92233720368547758.08"
        );
    }

    #[test]
    fn adds_and_subtracts_same_currency() {
        let code = eur().code();
        let a = Money::new(1000, code);
        let b = Money::new(250, code);
        assert_eq!(a.checked_add(b), Ok(Money::new(1250, code)));
        assert_eq!(a.checked_sub(b), Ok(Money::new(750, code)));
        assert_eq!(b.checked_sub(a), Ok(Money::new(-750, code)));
    }

    #[test]
    fn arithmetic_overflow_is_an_error() {
        let code = eur().code();
        let max = Money::new(i64::MAX, code);
        let min = Money::new(i64::MIN, code);
        let one = Money::new(1, code);
        assert_eq!(max.checked_add(one), Err(MoneyError::Overflow));
        assert_eq!(min.checked_sub(one), Err(MoneyError::Overflow));
    }

    #[test]
    fn mixing_currencies_is_an_error() {
        let eur = Money::new(100, eur().code());
        let usd = Money::new(100, currency("USD", 2).code());
        let mismatch = MoneyError::CurrencyMismatch {
            left: eur.currency(),
            right: usd.currency(),
        };
        assert_eq!(eur.checked_add(usd), Err(mismatch.clone()));
        assert_eq!(eur.checked_sub(usd), Err(mismatch.clone()));
        assert_eq!(
            mismatch.to_string(),
            "cannot combine amounts in EUR and USD"
        );
    }

    #[test]
    fn sign_helpers() {
        let code = eur().code();
        assert!(Money::new(1, code).is_positive());
        assert!(!Money::zero(code).is_positive());
        assert!(!Money::new(-1, code).is_positive());
        assert_eq!(Money::new(5, code).negated(), Money::new(-5, code));
        assert_eq!(Money::new(i64::MIN, code).negated().minor(), i64::MAX);
    }

    #[test]
    fn serialises_minor_units_and_code() {
        let money = Money::new(12345, eur().code());
        let json = serde_json::to_string(&money).unwrap();
        assert_eq!(json, r#"{"minor":12345,"currency":"EUR"}"#);
        assert_eq!(serde_json::from_str::<Money>(&json).unwrap(), money);
    }

    // `proptest!` generates many random inputs and checks the property for
    // each, shrinking any failure to the smallest example. Like Go's fuzzing.
    proptest! {
        #[test]
        fn parse_inverts_format(minor in (i64::MIN + 1)..=i64::MAX, places in 0u8..=8) {
            let currency = currency("EUR", places);
            let text = Money::new(minor, currency.code()).to_decimal_string(&currency);
            let parsed = Money::parse_decimal(&text, &currency).unwrap();
            prop_assert_eq!(parsed.minor(), minor);
        }

        #[test]
        fn add_then_sub_is_identity(a in (i64::MIN / 2)..(i64::MAX / 2), b in (i64::MIN / 2)..(i64::MAX / 2)) {
            let code = eur().code();
            let (a, b) = (Money::new(a, code), Money::new(b, code));
            prop_assert_eq!(a.checked_add(b).unwrap().checked_sub(b).unwrap(), a);
        }
    }
}
