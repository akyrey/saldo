//! Budget maths: what was spent against a limit, what is left, and the
//! limits an auto-budget creates for each new period (ARCHITECTURE.md 4.3).

use chrono::NaiveDate;

use crate::currency::CurrencyCode;
use crate::error::MoneyError;
use crate::money::Money;
use crate::period::{DateRange, Period};

/// How an auto-budget fills each new period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoBudgetType {
    /// Every period gets the same amount.
    Reset,
    /// Every period gets the amount plus what was left of the previous one
    /// (never less than the amount: overspending is not carried over).
    Rollover,
}

impl AutoBudgetType {
    pub fn as_str(self) -> &'static str {
        match self {
            AutoBudgetType::Reset => "reset",
            AutoBudgetType::Rollover => "rollover",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "reset" => Some(AutoBudgetType::Reset),
            "rollover" => Some(AutoBudgetType::Rollover),
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
/// the current period is created.
///
/// A rollover period adds what was left of the period before it, if that
/// limit was in the same currency and not overspent.
pub fn auto_limits(
    auto: &AutoBudget,
    last: Option<&Limit>,
    spends: &[Spend],
    today: NaiveDate,
) -> Result<Vec<Limit>, MoneyError> {
    let first = match last {
        // The period after the one `last` ends in (a hand-made limit may end
        // mid-period; the rest of that period stays covered by it).
        Some(limit) => match auto.period.range_of(limit.range.end).end.succ_opt() {
            Some(next) => next,
            None => return Ok(Vec::new()),
        },
        None => auto.period.start_of(today),
    };
    let mut previous = last.copied();
    let mut created = Vec::new();
    let mut start = first;
    while start <= today {
        let range = auto.period.range_of(start);
        let amount = match (auto.kind, previous) {
            (AutoBudgetType::Rollover, Some(prev))
                if prev.amount.currency() == auto.amount.currency() =>
            {
                let left = available(&prev, spent(&prev.range, prev.amount.currency(), spends)?)?;
                if left.is_positive() {
                    auto.amount.checked_add(left)?
                } else {
                    auto.amount
                }
            }
            _ => auto.amount,
        };
        let limit = Limit { range, amount };
        created.push(limit);
        previous = Some(limit);
        match range.end.succ_opt() {
            Some(next) => start = next,
            None => break,
        }
    }
    Ok(created)
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
        for kind in [AutoBudgetType::Reset, AutoBudgetType::Rollover] {
            assert_eq!(AutoBudgetType::parse(kind.as_str()), Some(kind));
        }
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
            &[],
            d(2026, 11, 13),
        )
        .unwrap();
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
        let spends = [spend(d(2026, 8, 3), eur(1_000))];
        let limits = auto_limits(
            &auto(AutoBudgetType::Reset, Period::Monthly),
            Some(&last),
            &spends,
            d(2026, 11, 13),
        )
        .unwrap();
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
        let budget = auto(AutoBudgetType::Rollover, Period::Monthly);
        assert!(
            auto_limits(&budget, Some(&last), &[], d(2026, 11, 30))
                .unwrap()
                .is_empty()
        );
        // A limit reaching past today, too.
        let future = Limit {
            range: month(2027, 1),
            amount: eur(1),
        };
        assert!(
            auto_limits(&budget, Some(&future), &[], d(2026, 11, 30))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn rollover_carries_what_was_left_but_not_overspending() {
        let last = Limit {
            range: month(2026, 9),
            amount: eur(30_000),
        };
        let spends = [
            spend(d(2026, 9, 10), eur(20_000)),  // 100.00 left in September
            spend(d(2026, 10, 10), eur(50_000)), // October overspent
        ];
        let limits = auto_limits(
            &auto(AutoBudgetType::Rollover, Period::Monthly),
            Some(&last),
            &spends,
            d(2026, 11, 2),
        )
        .unwrap();
        let amounts: Vec<_> = limits.iter().map(|l| l.amount).collect();
        // October: 300 + 100 left; November: 300 (October was overspent).
        assert_eq!(amounts, vec![eur(40_000), eur(30_000)]);
    }

    #[test]
    fn rollover_chains_leftovers_and_ignores_other_currencies() {
        let usd_last = Limit {
            range: month(2026, 9),
            amount: Money::new(10_000, code("USD")),
        };
        let budget = auto(AutoBudgetType::Rollover, Period::Monthly);
        let limits = auto_limits(&budget, Some(&usd_last), &[], d(2026, 11, 2)).unwrap();
        // The USD limit carries nothing into EUR; then October's unspent 300 does.
        let amounts: Vec<_> = limits.iter().map(|l| l.amount).collect();
        assert_eq!(amounts, vec![eur(30_000), eur(60_000)]);
    }

    #[test]
    fn a_limit_ending_mid_period_starts_the_next_full_period() {
        let last = Limit {
            range: DateRange::new(d(2026, 11, 1), d(2026, 11, 10)).unwrap(),
            amount: eur(5_000),
        };
        let weekly = auto(AutoBudgetType::Reset, Period::Weekly);
        // 10 November is a Tuesday: its week ends on Sunday the 15th.
        let limits = auto_limits(&weekly, Some(&last), &[], d(2026, 11, 20)).unwrap();
        assert_eq!(limits.len(), 1);
        assert_eq!(
            limits[0].range,
            DateRange::new(d(2026, 11, 16), d(2026, 11, 22)).unwrap()
        );
    }

    #[test]
    fn rollover_overflow_is_an_error() {
        let last = Limit {
            range: month(2026, 10),
            amount: eur(i64::MAX),
        };
        let budget = auto(AutoBudgetType::Rollover, Period::Monthly);
        assert_eq!(
            auto_limits(&budget, Some(&last), &[], d(2026, 11, 2)),
            Err(MoneyError::Overflow)
        );
    }
}
