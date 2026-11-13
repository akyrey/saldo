//! Firefly sends amounts with 12 decimals (`"185.000000000000"`) whatever the
//! currency. Saldo wants exactly the currency's places (`"185.00"`).

/// Trims zero padding to `places` decimals. A non-zero digit beyond them
/// means the amount cannot be represented exactly, which is an error.
///
/// `&str` is a borrowed view of text (like a Go string slice into a buffer);
/// the returned `String` is newly owned text the caller keeps.
pub fn normalize(text: &str, places: u8) -> Result<String, String> {
    let text = text.trim();
    let (sign, digits) = match text.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", text),
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    let all_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if whole.is_empty() || !all_digits(whole) || !all_digits(fraction) {
        return Err(format!("{text:?} is not a decimal amount"));
    }
    let places = usize::from(places);
    let (kept, dropped) = fraction.split_at(fraction.len().min(places));
    if dropped.bytes().any(|b| b != b'0') {
        return Err(format!("{text:?} has more than {places} decimal places"));
    }
    if places == 0 {
        return Ok(format!("{sign}{whole}"));
    }
    Ok(format!("{sign}{whole}.{kept:0<places$}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_padding_to_the_currency_places() {
        assert_eq!(normalize("185.000000000000", 2).unwrap(), "185.00");
        assert_eq!(normalize("42.500000000000", 2).unwrap(), "42.50");
        assert_eq!(normalize("3.000000000000", 0).unwrap(), "3");
        assert_eq!(normalize("-0.500000000000", 2).unwrap(), "-0.50");
        assert_eq!(normalize("7", 2).unwrap(), "7.00");
        assert_eq!(normalize("1.5", 3).unwrap(), "1.500");
    }

    #[test]
    fn rejects_precision_loss_and_garbage() {
        assert!(normalize("1.234000000000", 2).is_err());
        assert!(normalize("1.5", 0).is_err());
        for bad in ["", "abc", "1,5", ".5", "1.2.3"] {
            assert!(normalize(bad, 2).is_err(), "{bad}");
        }
    }
}
