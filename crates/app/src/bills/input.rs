//! Input types for bills and their checks.

use chrono::NaiveDate;
use saldo_core::{BillFrequency, Currency, Money};

use crate::error::AppError;

/// A new bill. `currency` defaults to the household's primary currency.
#[derive(Debug, Clone)]
pub struct NewBill {
    pub name: String,
    /// Decimal strings; `amount_min <= amount_max`.
    pub amount_min: String,
    pub amount_max: String,
    pub currency: Option<String>,
    pub first_due_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    /// `weekly`, `monthly`, `quarterly`, `half_year` or `yearly`.
    pub repeat_freq: String,
    pub skip: Option<u32>,
    pub remind_days_before: Option<u32>,
    pub remind_every_days: Option<u32>,
    pub active: Option<bool>,
    pub sort_order: Option<i64>,
    pub notes: Option<String>,
}

/// Changes to a bill; `None` keeps a value, `Some(None)` clears a nullable
/// one. The currency cannot change.
#[derive(Debug, Clone, Default)]
pub struct BillUpdate {
    pub name: Option<String>,
    pub amount_min: Option<String>,
    pub amount_max: Option<String>,
    pub first_due_date: Option<NaiveDate>,
    pub end_date: Option<Option<NaiveDate>>,
    pub repeat_freq: Option<String>,
    pub skip: Option<u32>,
    pub remind_days_before: Option<u32>,
    pub remind_every_days: Option<u32>,
    pub active: Option<bool>,
    pub sort_order: Option<i64>,
    pub notes: Option<Option<String>>,
}

pub(crate) const DEFAULT_REMIND_DAYS_BEFORE: u32 = 3;
pub(crate) const DEFAULT_REMIND_EVERY_DAYS: u32 = 1;

pub(crate) fn parse_frequency(text: &str) -> Result<BillFrequency, AppError> {
    BillFrequency::parse(text).ok_or_else(|| {
        AppError::invalid(
            "repeat_freq",
            "must be weekly, monthly, quarterly, half_year or yearly",
        )
    })
}

/// Parses both amounts in the bill's currency and checks `0 <= min <= max`.
pub(crate) fn parse_amounts(
    min: &str,
    max: &str,
    currency: &Currency,
) -> Result<(i64, i64), AppError> {
    let parse = |field: &str, text: &str| {
        Money::parse_decimal(text, currency)
            .map(|m| m.minor())
            .map_err(|e| AppError::invalid(field, e.to_string()))
    };
    let (min, max) = (parse("amount_min", min)?, parse("amount_max", max)?);
    if min < 0 {
        return Err(AppError::invalid("amount_min", "must not be negative"));
    }
    if min > max {
        return Err(AppError::invalid(
            "amount_max",
            "must be at least amount_min",
        ));
    }
    Ok((min, max))
}

pub(crate) fn check_dates(
    first_due_date: NaiveDate,
    end_date: Option<NaiveDate>,
) -> Result<(), AppError> {
    if end_date.is_some_and(|end| end < first_due_date) {
        return Err(AppError::invalid(
            "end_date",
            "must not be before first_due_date",
        ));
    }
    Ok(())
}

pub(crate) fn check_every(days: u32) -> Result<(), AppError> {
    if days == 0 {
        return Err(AppError::invalid("remind_every_days", "must be at least 1"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use saldo_core::CurrencyCode;

    fn eur() -> Currency {
        Currency::new(CurrencyCode::new("EUR").unwrap(), 2).unwrap()
    }

    #[test]
    fn amounts_are_ordered_and_not_negative() {
        assert_eq!(parse_amounts("9.99", "12.99", &eur()).unwrap(), (999, 1299));
        assert_eq!(parse_amounts("0", "0", &eur()).unwrap(), (0, 0));
        assert!(parse_amounts("-1", "5", &eur()).is_err());
        assert!(parse_amounts("10", "5", &eur()).is_err());
        assert!(parse_amounts("1.234", "5", &eur()).is_err());
    }

    #[test]
    fn dates_frequency_and_reminder_interval_are_checked() {
        let day = |t: &str| t.parse::<NaiveDate>().unwrap();
        assert!(check_dates(day("2026-01-01"), Some(day("2025-12-31"))).is_err());
        assert!(check_dates(day("2026-01-01"), None).is_ok());
        assert!(parse_frequency("monthly").is_ok());
        assert!(parse_frequency("daily").is_err());
        assert!(check_every(0).is_err());
        assert!(check_every(1).is_ok());
    }
}
