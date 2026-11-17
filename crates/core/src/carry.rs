//! Budget carry-over: what one limit passes on to the next (ARCHITECTURE.md
//! 4.3). Computed whenever limits are read, never stored, so a transaction
//! edited in an earlier period changes every later limit at once.

use std::collections::HashMap;

use chrono::NaiveDate;

use crate::budget::{Limit, Spend, spent};
use crate::currency::CurrencyCode;
use crate::error::MoneyError;
use crate::money::Money;

/// What a budget carries from one limit into the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carry {
    /// Every limit starts fresh.
    None,
    /// What was left over carries; overspending is forgotten.
    Leftover,
    /// What was left over carries, and so does overspending (as a negative
    /// amount that shrinks the next limit).
    All,
}

impl Carry {
    pub fn as_str(self) -> &'static str {
        match self {
            Carry::None => "none",
            Carry::Leftover => "leftover",
            Carry::All => "all",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "none" => Some(Carry::None),
            "leftover" => Some(Carry::Leftover),
            "all" => Some(Carry::All),
            _ => None,
        }
    }

    /// The part of `left` (what remained of a limit) passed to the next one.
    fn of(self, left: Money) -> Money {
        match self {
            Carry::Leftover if left.is_positive() => left,
            Carry::All => left,
            _ => Money::zero(left.currency()),
        }
    }
}

/// The amount each limit receives from the one before it, in the limit's
/// currency, in the same order as `limits`.
///
/// `limits` are one budget's limits sorted by start date (several currencies
/// may be mixed; each currency is its own chain). A limit receives a carry
/// only when it starts the day after the previous limit in its currency ends;
/// after a gap it starts from zero again. What a limit passes on is
/// `amount + carried - spent`, filtered by `carry`.
pub fn carried(limits: &[Limit], spends: &[Spend], carry: Carry) -> Result<Vec<Money>, MoneyError> {
    // Per currency: the previous limit's end date and what was left of it.
    // Like a Go `map[string]struct{...}`; `HashMap` needs the key to be
    // hashable, which `CurrencyCode` derives.
    let mut previous: HashMap<CurrencyCode, (NaiveDate, Money)> = HashMap::new();
    let mut result = Vec::with_capacity(limits.len());
    for limit in limits {
        let currency = limit.amount.currency();
        let carried_in = match previous.get(&currency) {
            Some((end, left)) if end.succ_opt() == Some(limit.range.start) => carry.of(*left),
            _ => Money::zero(currency),
        };
        let spent = spent(&limit.range, currency, spends)?;
        let left = limit.amount.checked_add(carried_in)?.checked_sub(spent)?;
        previous.insert(currency, (limit.range.end, left));
        result.push(carried_in);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::period::{DateRange, Period};
    use proptest::prelude::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn eur(minor: i64) -> Money {
        Money::new(minor, CurrencyCode::new("EUR").unwrap())
    }

    fn usd(minor: i64) -> Money {
        Money::new(minor, CurrencyCode::new("USD").unwrap())
    }

    fn month(y: i32, m: u32, amount: Money) -> Limit {
        Limit {
            range: Period::Monthly.range_of(d(y, m, 1)),
            amount,
        }
    }

    fn spend(date: NaiveDate, amount: Money) -> Spend {
        Spend {
            date,
            amount,
            foreign_amount: None,
        }
    }

    #[test]
    fn names_round_trip() {
        for carry in [Carry::None, Carry::Leftover, Carry::All] {
            assert_eq!(Carry::parse(carry.as_str()), Some(carry));
        }
        assert_eq!(Carry::parse("rollover"), None);
    }

    #[test]
    fn none_carries_nothing() {
        let limits = [month(2026, 9, eur(300)), month(2026, 10, eur(300))];
        let result = carried(&limits, &[], Carry::None).unwrap();
        assert_eq!(result, vec![eur(0), eur(0)]);
    }

    #[test]
    fn leftover_chains_what_was_left_but_not_overspending() {
        let limits = [
            month(2026, 9, eur(300)),
            month(2026, 10, eur(300)),
            month(2026, 11, eur(300)),
        ];
        let spends = [
            spend(d(2026, 9, 10), eur(200)),  // 100 left in September
            spend(d(2026, 10, 10), eur(500)), // October: 400 available, 100 overspent
        ];
        let result = carried(&limits, &spends, Carry::Leftover).unwrap();
        assert_eq!(result, vec![eur(0), eur(100), eur(0)]);
    }

    #[test]
    fn all_carries_overspending_as_a_negative_amount() {
        let limits = [
            month(2026, 9, eur(300)),
            month(2026, 10, eur(300)),
            month(2026, 11, eur(300)),
        ];
        let spends = [
            spend(d(2026, 9, 10), eur(450)),  // 150 overspent
            spend(d(2026, 10, 10), eur(100)), // 300 - 150 - 100 = 50 left
        ];
        let result = carried(&limits, &spends, Carry::All).unwrap();
        assert_eq!(result, vec![eur(0), eur(-150), eur(50)]);
    }

    #[test]
    fn a_gap_resets_the_carry() {
        let limits = [month(2026, 8, eur(300)), month(2026, 10, eur(300))];
        let result = carried(&limits, &[], Carry::All).unwrap();
        assert_eq!(result, vec![eur(0), eur(0)]);
    }

    #[test]
    fn hand_made_ranges_chain_when_they_touch() {
        let first = Limit {
            range: DateRange::new(d(2026, 11, 1), d(2026, 11, 10)).unwrap(),
            amount: eur(100),
        };
        let second = Limit {
            range: DateRange::new(d(2026, 11, 11), d(2026, 11, 25)).unwrap(),
            amount: eur(100),
        };
        let result = carried(&[first, second], &[], Carry::Leftover).unwrap();
        assert_eq!(result, vec![eur(0), eur(100)]);
    }

    #[test]
    fn each_currency_is_its_own_chain() {
        let limits = [
            month(2026, 9, eur(300)),
            month(2026, 9, usd(50)),
            month(2026, 10, eur(300)),
            month(2026, 10, usd(50)),
        ];
        let spends = [spend(d(2026, 9, 3), usd(20))];
        let result = carried(&limits, &spends, Carry::Leftover).unwrap();
        assert_eq!(result, vec![eur(0), usd(0), eur(300), usd(30)]);
    }

    #[test]
    fn overflow_is_an_error() {
        let limits = [month(2026, 9, eur(i64::MAX)), month(2026, 10, eur(1))];
        assert_eq!(
            carried(&limits, &[], Carry::Leftover),
            Err(MoneyError::Overflow)
        );
    }

    proptest! {
        /// With `All`, what is left at the end of a gap-free chain is every
        /// amount minus everything spent; with `Leftover`, no carry is negative.
        #[test]
        fn the_chain_balances(
            months in prop::collection::vec((1i64..1_000_000, 0i64..2_000_000), 1..24)
        ) {
            let start = d(2025, 1, 1);
            let mut limits = Vec::new();
            let mut spends = Vec::new();
            let mut first = start;
            for (amount, spent_minor) in &months {
                let range = Period::Monthly.range_of(first);
                limits.push(Limit { range, amount: eur(*amount) });
                spends.push(spend(range.start, eur(*spent_minor)));
                first = range.end.succ_opt().unwrap();
            }
            let all = carried(&limits, &spends, Carry::All).unwrap();
            let last = limits.len() - 1;
            let (amount, spent_minor) = months[last];
            let left = all[last].minor() + amount - spent_minor;
            let total: i64 = months.iter().map(|(a, s)| a - s).sum();
            prop_assert_eq!(left, total);

            let leftover = carried(&limits, &spends, Carry::Leftover).unwrap();
            prop_assert!(leftover.iter().all(|m| m.minor() >= 0));
        }
    }
}
