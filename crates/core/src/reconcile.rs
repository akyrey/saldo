//! Reconciliation: comparing an account with a bank statement
//! (ARCHITECTURE.md 4.3, Phase 9).

use crate::error::MoneyError;
use crate::money::Money;

/// What the statement says minus what the cleared amounts add up to: `None`
/// when they agree. `cleared` are the signed amounts (+ in, - out, in the
/// account's currency) of every split the bank has confirmed up to the
/// statement date; all must be in the statement balance's currency.
pub fn reconcile(statement_balance: Money, cleared: &[Money]) -> Result<Option<Money>, MoneyError> {
    let currency = statement_balance.currency();
    let total = cleared
        .iter()
        .try_fold(Money::zero(currency), |sum, amount| {
            sum.checked_add(*amount)
        })?;
    let difference = statement_balance.checked_sub(total)?;
    Ok((difference.minor() != 0).then_some(difference))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::currency::CurrencyCode;

    fn eur(minor: i64) -> Money {
        Money::new(minor, CurrencyCode::new("EUR").unwrap())
    }

    #[test]
    fn agrees_when_the_cleared_amounts_add_up() {
        let cleared = [eur(100_000), eur(-2_550), eur(-7_450)];
        assert_eq!(reconcile(eur(90_000), &cleared).unwrap(), None);
    }

    #[test]
    fn the_difference_is_what_the_statement_has_more() {
        let cleared = [eur(100_000), eur(-2_550)];
        assert_eq!(reconcile(eur(97_000), &cleared).unwrap(), Some(eur(-450)));
        assert_eq!(reconcile(eur(0), &[]).unwrap(), None);
        assert_eq!(reconcile(eur(-5), &[]).unwrap(), Some(eur(-5)));
    }

    #[test]
    fn currencies_and_overflow_are_checked() {
        let usd = Money::new(1, CurrencyCode::new("USD").unwrap());
        assert!(matches!(
            reconcile(eur(0), &[usd]),
            Err(MoneyError::CurrencyMismatch { .. })
        ));
        assert_eq!(
            reconcile(eur(0), &[eur(i64::MAX), eur(1)]),
            Err(MoneyError::Overflow)
        );
    }
}
