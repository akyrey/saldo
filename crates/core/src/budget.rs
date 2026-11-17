//! Budget maths: what was spent against a limit, what is left, and the
//! limits an auto-budget creates for each new period (ARCHITECTURE.md 4.3).
//! What one limit passes on to the next is in `carry`.

use chrono::NaiveDate;

use crate::currency::CurrencyCode;
use crate::error::MoneyError;
use crate::money::Money;
use crate::period::{DateRange, Period};

/// How an auto-budget fills each new period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoBudgetType {
    /// Every period gets the same amount. (Carrying leftovers is the
    /// budget's `Carry`, computed when limits are read, not stored here.)
    Reset,
}

impl AutoBudgetType {
    pub fn as_str(self) -> &'static str {
        match self {
            AutoBudgetType::Reset => "reset",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "reset" => Some(AutoBudgetType::Reset),
            _ => None,
        }
    }
}

/// A budget's automatic limit: `amount` per `period`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoBudget {
    pub kind: AutoBudgetType,
    pub period: Period,
    pub amount: Money,
}

/// An amount a budget may spend between two dates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limit {
    pub range: DateRange,
    pub amount: Money,
}

/// One withdrawal booked on a budget: its date, amount and optional
/// foreign amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spend {
    pub date: NaiveDate,
    pub amount: Money,
    pub foreign_amount: Option<Money>,
}

impl Spend {
    /// The amount in `currency`: the amount itself, or the foreign amount
    /// when that is in `currency`; `None` when neither is.
    fn in_currency(&self, currency: CurrencyCode) -> Option<Money> {
        if self.amount.currency() == currency {
            return Some(self.amount);
        }
        self.foreign_amount.filter(|f| f.currency() == currency)
    }
}

/// The total of the spends dated inside `range` that are in `currency`
/// (directly or through their foreign amount). Others are ignored: a limit in
/// EUR knows nothing about a payment made only in USD.
pub fn spent(
    range: &DateRange,
    currency: CurrencyCode,
    spends: &[Spend],
) -> Result<Money, MoneyError> {
    // An iterator chain is lazy like a PHP generator pipeline: nothing runs
    // until `try_fold` pulls the values through, one at a time.
    spends
        .iter()
        .filter(|s| range.contains(s.date))
        .filter_map(|s| s.in_currency(currency))
        .try_fold(Money::zero(currency), |total, amount| {
            total.checked_add(amount)
        })
}

/// What is left of `limit` after `spent`; negative when overspent.
pub fn available(limit: &Limit, spent: Money) -> Result<Money, MoneyError> {
    limit.amount.checked_sub(spent)
}

/// The limits an auto-budget still has to create, oldest first: one per
/// period, from the period after `last` (the budget's latest limit) up to and
/// including the period that contains `today`. Without a `last` limit, only
/// the current period is created. Every limit gets the auto-budget's amount.
pub fn auto_limits(auto: &AutoBudget, last: Option<&Limit>, today: NaiveDate) -> Vec<Limit> {
    let first = match last {
        // The period after the one `last` ends in (a hand-made limit may end
        // mid-period; the rest of that period stays covered by it).
        Some(limit) => match auto.period.range_of(limit.range.end).end.succ_opt() {
            Some(next) => next,
            None => return Vec::new(),
        },
        None => auto.period.start_of(today),
    };
    let mut created = Vec::new();
    let mut start = first;
    while start <= today {
        let range = auto.period.range_of(start);
        created.push(Limit {
            range,
            amount: auto.amount,
        });
        match range.end.succ_opt() {
            Some(next) => start = next,
            None => break,
        }
    }
    created
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn code(text: &str) -> CurrencyCode {
        CurrencyCode::new(text).unwrap()
    }

    fn eur(minor: i64) -> Money {
        Money::new(minor, code("EUR"))
    }

    fn spend(date: NaiveDate, amount: Money) -> Spend {
        Spend {
            date,
            amount,
            foreign_amount: None,
        }
    }

    fn month(y: i32, m: u32) -> DateRange {
        Period::Monthly.range_of(d(y, m, 1))
    }

    #[test]
    fn type_names_round_trip() {
        assert_eq!(
            AutoBudgetType::parse(AutoBudgetType::Reset.as_str()),
            Some(AutoBudgetType::Reset)
        );
        // Rollover is now the budget's carry, not an auto-budget type.
        assert_eq!(AutoBudgetType::parse("rollover"), None);
        assert_eq!(AutoBudgetType::parse("adjusted"), None);
    }

    #[test]
    fn spent_counts_only_the_range_and_the_currency() {
        let usd = code("USD");
        let spends = [
            spend(d(2026, 10, 31), eur(1_000)), // before
            spend(d(2026, 11, 1), eur(250)),
            spend(d(2026, 11, 30), eur(750)),
            spend(d(2026, 12, 1), eur(5_000)),            // after
            spend(d(2026, 11, 15), Money::new(999, usd)), // other currency
            Spend {
                // USD payment with a EUR foreign amount counts in EUR.
                date: d(2026, 11, 20),
                amount: Money::new(1_100, usd),
                foreign_amount: Some(eur(1_000)),
            },
        ];
        let total = spent(&month(2026, 11), code("EUR"), &spends).unwrap();
        assert_eq!(total, eur(2_000));
        let in_usd = spent(&month(2026, 11), usd, &spends).unwrap();
        assert_eq!(in_usd, Money::new(2_099, usd));
        assert_eq!(
            spent(&month(2026, 9), code("EUR"), &spends).unwrap(),
            eur(0)
        );
    }

    #[test]
    fn spent_reports_overflow() {
        let spends = [
            spend(d(2026, 11, 1), eur(i64::MAX)),
            spend(d(2026, 11, 2), eur(1)),
        ];
        assert_eq!(
            spent(&month(2026, 11), code("EUR"), &spends),
            Err(MoneyError::Overflow)
        );
    }

    #[test]
    fn available_is_limit_minus_spent() {
        let limit = Limit {
            range: month(2026, 11),
            amount: eur(30_000),
        };
        assert_eq!(available(&limit, eur(12_000)).unwrap(), eur(18_000));
        assert_eq!(available(&limit, eur(31_000)).unwrap(), eur(-1_000));
    }

    fn auto(kind: AutoBudgetType, period: Period) -> AutoBudget {
        AutoBudget {
            kind,
            period,
            amount: eur(30_000),
        }
    }

    #[test]
    fn without_a_limit_only_the_current_period_is_created() {
        let limits = auto_limits(
            &auto(AutoBudgetType::Reset, Period::Monthly),
            None,
            d(2026, 11, 13),
        );
        assert_eq!(
            limits,
            vec![Limit {
                range: month(2026, 11),
                amount: eur(30_000)
            }]
        );
    }

    #[test]
    fn reset_fills_every_missing_period_with_the_amount() {
        let last = Limit {
            range: month(2026, 8),
            amount: eur(10_000),
        };
        let limits = auto_limits(
            &auto(AutoBudgetType::Reset, Period::Monthly),
            Some(&last),
            d(2026, 11, 13),
        );
        let ranges: Vec<_> = limits.iter().map(|l| l.range).collect();
        assert_eq!(
            ranges,
            vec![month(2026, 9), month(2026, 10), month(2026, 11)]
        );
        assert!(limits.iter().all(|l| l.amount == eur(30_000)));
    }

    #[test]
    fn nothing_is_created_when_the_current_period_is_covered() {
        let last = Limit {
            range: month(2026, 11),
            amount: eur(30_000),
        };
        let budget = auto(AutoBudgetType::Reset, Period::Monthly);
        assert!(auto_limits(&budget, Some(&last), d(2026, 11, 30)).is_empty());
        // A limit reaching past today, too.
        let future = Limit {
            range: month(2027, 1),
            amount: eur(1),
        };
        assert!(auto_limits(&budget, Some(&future), d(2026, 11, 30)).is_empty());
    }

    #[test]
    fn a_limit_ending_mid_period_starts_the_next_full_period() {
        let last = Limit {
            range: DateRange::new(d(2026, 11, 1), d(2026, 11, 10)).unwrap(),
            amount: eur(5_000),
        };
        let weekly = auto(AutoBudgetType::Reset, Period::Weekly);
        // 10 November is a Tuesday: its week ends on Sunday the 15th.
        let limits = auto_limits(&weekly, Some(&last), d(2026, 11, 20));
        assert_eq!(limits.len(), 1);
        assert_eq!(
            limits[0].range,
            DateRange::new(d(2026, 11, 16), d(2026, 11, 22)).unwrap()
        );
    }
}
