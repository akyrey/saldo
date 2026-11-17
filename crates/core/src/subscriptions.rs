//! Subscription detection: repeating payments to the same counterparty that
//! look like a bill nobody has set up yet (ARCHITECTURE.md 4.3, Phase 9).

use std::collections::BTreeMap;

use chrono::{Days, Months, NaiveDate};

use crate::bills::BillFrequency;
use crate::currency::CurrencyCode;
use crate::ids::{AccountId, SplitId};
use crate::money::Money;

/// One withdrawal to an expense account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Payment {
    pub split_id: SplitId,
    pub date: NaiveDate,
    pub amount: Money,
    /// The expense account the money went to.
    pub counterparty: AccountId,
    /// Already linked to a bill: the counterparty is taken care of.
    pub has_bill: bool,
}

/// A repeating payment that could become a bill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub counterparty: AccountId,
    pub frequency: BillFrequency,
    pub amount_min: Money,
    pub amount_max: Money,
    pub first_payment: NaiveDate,
    pub last_payment: NaiveDate,
    /// One period after the last payment's scheduled date.
    pub next_due: NaiveDate,
    /// The matched payments, oldest first.
    pub split_ids: Vec<SplitId>,
    /// The share of gaps between payments that fit the frequency, 0 to 100.
    pub confidence: u8,
}

/// Per frequency, shortest first: how far a payment may land from its
/// expected date, and how many payments it takes to call it a pattern.
const FREQUENCIES: [(BillFrequency, u64, usize); 5] = [
    (BillFrequency::Weekly, 2, 3),
    (BillFrequency::Monthly, 4, 3),
    (BillFrequency::Quarterly, 7, 3),
    (BillFrequency::HalfYear, 10, 2),
    (BillFrequency::Yearly, 15, 2),
];

/// Amounts must be within 1/5 (20%) of the latest payment's amount, so a
/// price change keeps the newer payments together.
const AMOUNT_SPREAD_DIVISOR: i128 = 5;

/// Repeating payments in `payments`, grouped by counterparty and currency.
/// A group with any payment already linked to a bill is left out. Only
/// payments within 20% of the latest amount count; the first frequency
/// (shortest first) where at least three quarters of the gaps land within the
/// tolerance of one period wins, if the last payment is recent enough that the
/// next one is not overdue past the tolerance on `today`. Sorted by next due
/// date, then counterparty.
pub fn detect_subscriptions(payments: &[Payment], today: NaiveDate) -> Vec<Candidate> {
    // A `BTreeMap` (sorted keys) keeps the output stable between runs.
    let mut groups: BTreeMap<(AccountId, CurrencyCode), Vec<Payment>> = BTreeMap::new();
    for payment in payments {
        groups
            .entry((payment.counterparty, payment.amount.currency()))
            .or_default()
            .push(*payment);
    }
    let mut candidates: Vec<Candidate> = groups
        .into_values()
        .filter(|group| !group.iter().any(|p| p.has_bill))
        .filter_map(|group| detect_group(group, today))
        .collect();
    candidates.sort_by_key(|c| (c.next_due, c.counterparty));
    candidates
}

fn detect_group(mut group: Vec<Payment>, today: NaiveDate) -> Option<Candidate> {
    group.sort_by_key(|p| p.date);
    let latest = group.last()?.amount.minor();
    let kept: Vec<Payment> = group
        .into_iter()
        .filter(|p| {
            let difference = i128::from(p.amount.minor()) - i128::from(latest);
            difference.abs() * AMOUNT_SPREAD_DIVISOR <= i128::from(latest).abs()
        })
        .collect();
    FREQUENCIES
        .iter()
        .find_map(|&(frequency, tolerance, needed)| {
            fits(&kept, frequency, tolerance, needed, today)
        })
}

/// The candidate if `payments` (sorted, all close in amount) follow `frequency`.
fn fits(
    payments: &[Payment],
    frequency: BillFrequency,
    tolerance: u64,
    needed: usize,
    today: NaiveDate,
) -> Option<Candidate> {
    if payments.len() < needed {
        return None;
    }
    let gaps = payments.len() - 1;
    // Each payment is compared with the date the schedule expected, not the
    // previous payment's actual date, so a late month followed by an early
    // one does not look like a gap twice as wide. A payment that misses
    // starts the schedule again from its own date.
    let mut expected_last = payments.first()?.date;
    let mut fitting = 0;
    for payment in &payments[1..] {
        let expected = next_after(frequency, expected_last)?;
        if days_apart(expected, payment.date) <= tolerance {
            fitting += 1;
            expected_last = expected;
        } else {
            expected_last = payment.date;
        }
    }
    if fitting * 4 < gaps * 3 {
        return None;
    }
    let (first, last) = (payments.first()?, payments.last()?);
    let next_due = next_after(frequency, expected_last)?;
    if today > next_due.checked_add_days(Days::new(tolerance))? {
        return None; // stopped paying
    }
    let amounts = payments.iter().map(|p| p.amount);
    Some(Candidate {
        counterparty: last.counterparty,
        frequency,
        amount_min: amounts.clone().min_by_key(|m| m.minor())?,
        amount_max: amounts.max_by_key(|m| m.minor())?,
        first_payment: first.date,
        last_payment: last.date,
        next_due,
        split_ids: payments.iter().map(|p| p.split_id).collect(),
        // At most 100, so the conversion cannot fail.
        confidence: u8::try_from(fitting * 100 / gaps).unwrap_or(100),
    })
}

/// The date one period after `date`; a month-end is clamped (31 Jan → 28 Feb).
fn next_after(frequency: BillFrequency, date: NaiveDate) -> Option<NaiveDate> {
    let months = match frequency {
        BillFrequency::Weekly => return date.checked_add_days(Days::new(7)),
        BillFrequency::Monthly => 1,
        BillFrequency::Quarterly => 3,
        BillFrequency::HalfYear => 6,
        BillFrequency::Yearly => 12,
    };
    date.checked_add_months(Months::new(months))
}

fn days_apart(a: NaiveDate, b: NaiveDate) -> u64 {
    (a - b).num_days().unsigned_abs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn account(n: u128) -> AccountId {
        AccountId(Uuid::from_u128(n))
    }

    fn pay(n: u128, to: u128, date: NaiveDate, minor: i64) -> Payment {
        Payment {
            split_id: SplitId(Uuid::from_u128(n)),
            date,
            amount: Money::new(minor, CurrencyCode::new("EUR").unwrap()),
            counterparty: account(to),
            has_bill: false,
        }
    }

    /// A payment of `minor` to account 1 on day `day` of each (year, month).
    fn monthly(day: u32, minor: i64, months: &[(i32, u32)]) -> Vec<Payment> {
        months
            .iter()
            .enumerate()
            .map(|(n, &(y, m))| pay(n as u128 + 1, 1, d(y, m, day), minor))
            .collect()
    }

    const TODAY: (i32, u32, u32) = (2026, 11, 17);

    fn today() -> NaiveDate {
        d(TODAY.0, TODAY.1, TODAY.2)
    }

    #[test]
    fn finds_a_monthly_payment_with_a_late_month() {
        let mut payments = monthly(10, 1_299, &[(2026, 7), (2026, 8), (2026, 9), (2026, 11)]);
        payments[2].date = d(2026, 9, 13); // three days late still fits
        payments.insert(3, pay(9, 1, d(2026, 10, 8), 1_299));
        let found = detect_subscriptions(&payments, today());
        assert_eq!(found.len(), 1);
        let c = &found[0];
        assert_eq!(c.frequency, BillFrequency::Monthly);
        assert_eq!(
            (c.first_payment, c.last_payment),
            (d(2026, 7, 10), d(2026, 11, 10))
        );
        assert_eq!(c.next_due, d(2026, 12, 10));
        assert_eq!(c.split_ids.len(), 5);
        assert_eq!(c.confidence, 100);
    }

    #[test]
    fn a_price_rise_keeps_the_recent_payments() {
        // 9.99 for a long time, then 12.99: the old price is more than 20% off.
        let mut payments = monthly(1, 999, &[(2026, 3), (2026, 4), (2026, 5), (2026, 6)]);
        payments.extend(
            monthly(
                1,
                1_299,
                &[(2026, 7), (2026, 8), (2026, 9), (2026, 10), (2026, 11)],
            )
            .into_iter()
            .map(|p| Payment {
                split_id: SplitId(Uuid::from_u128(100 + p.split_id.0.as_u128())),
                ..p
            }),
        );
        let found = detect_subscriptions(&payments, today());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].split_ids.len(), 5);
        assert_eq!(found[0].amount_min.minor(), 1_299);
        assert_eq!(found[0].first_payment, d(2026, 7, 1));
    }

    #[test]
    fn finds_yearly_and_weekly_payments() {
        let yearly = [
            pay(1, 2, d(2025, 1, 20), 9_000),
            pay(2, 2, d(2026, 1, 25), 9_500),
        ];
        let weekly: Vec<_> = (0..4)
            .map(|n| pay(10 + n, 3, d(2026, 10, 20) + Days::new(7 * n as u64), 500))
            .collect();
        let all: Vec<_> = yearly.iter().copied().chain(weekly).collect();
        let found = detect_subscriptions(&all, today());
        let kinds: Vec<_> = found
            .iter()
            .map(|c| (c.counterparty, c.frequency))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (account(3), BillFrequency::Weekly),
                (account(2), BillFrequency::Yearly)
            ]
        );
        assert_eq!(found[1].amount_max.minor(), 9_500);
        assert_eq!(found[1].next_due, d(2027, 1, 20)); // the schedule, not the late payment
    }

    #[test]
    fn irregular_stopped_billed_and_too_few_are_left_out() {
        // Groceries: same shop, amounts all over the place, irregular days.
        let groceries = [
            pay(1, 1, d(2026, 9, 2), 4_000),
            pay(2, 1, d(2026, 9, 9), 8_250),
            pay(3, 1, d(2026, 9, 30), 4_100),
            pay(4, 1, d(2026, 10, 21), 3_900),
            pay(5, 1, d(2026, 11, 14), 4_050),
        ];
        // A gym cancelled in July.
        let stopped: Vec<_> = monthly(5, 3_000, &[(2026, 4), (2026, 5), (2026, 6)])
            .into_iter()
            .map(|p| Payment {
                counterparty: account(2),
                ..p
            })
            .collect();
        // Rent already has a bill.
        let mut billed: Vec<_> = monthly(1, 80_000, &[(2026, 9), (2026, 10), (2026, 11)])
            .into_iter()
            .map(|p| Payment {
                counterparty: account(3),
                ..p
            })
            .collect();
        billed[0].has_bill = true;
        // Only two monthly payments.
        let few: Vec<_> = monthly(3, 700, &[(2026, 10), (2026, 11)])
            .into_iter()
            .map(|p| Payment {
                counterparty: account(4),
                ..p
            })
            .collect();
        let all: Vec<_> = groceries
            .iter()
            .copied()
            .chain(stopped)
            .chain(billed)
            .chain(few)
            .collect();
        assert_eq!(detect_subscriptions(&all, today()), Vec::new());
    }

    #[test]
    fn one_odd_gap_in_four_is_tolerated_with_lower_confidence() {
        let mut payments = monthly(
            15,
            2_000,
            &[(2026, 6), (2026, 7), (2026, 8), (2026, 9), (2026, 10)],
        );
        payments[2].date = d(2026, 8, 25); // ten days late: two gaps miss
        let found = detect_subscriptions(&payments, today());
        assert!(found.is_empty(), "two of four gaps missing is too many");

        payments[2].date = d(2026, 8, 15);
        payments[4].date = d(2026, 10, 27); // only the last gap misses
        let found = detect_subscriptions(&payments, today());
        assert_eq!(found[0].confidence, 75);
    }
}
