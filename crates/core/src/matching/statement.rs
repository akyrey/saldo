//! Checks on a statement itself (L27) and splitting an Amazon charge by item
//! (decision 9).

use chrono::NaiveDate;

use crate::error::MoneyError;
use crate::money::Money;

/// `opening + sum(amounts) − closing`: what the statement's lines do not
/// explain, `None` when they explain it all. A gap means the export is
/// truncated or holds pending lines. All amounts in one currency.
pub fn statement_gap(
    opening: Money,
    amounts: &[Money],
    closing: Money,
) -> Result<Option<Money>, MoneyError> {
    let total = amounts
        .iter()
        .try_fold(opening, |sum, amount| sum.checked_add(*amount))?;
    let gap = total.checked_sub(closing)?;
    Ok((gap.minor() != 0).then_some(gap))
}

/// The balance at the end of each of `dates`, walking back from the
/// statement's `closing` balance: `closing` minus every line dated after
/// that date. Lines are `(booked date, signed amount)`.
pub fn balances_at(
    closing: Money,
    lines: &[(NaiveDate, Money)],
    dates: &[NaiveDate],
) -> Result<Vec<Money>, MoneyError> {
    dates
        .iter()
        .map(|date| {
            lines
                .iter()
                .filter(|(booked, _)| booked > date)
                .try_fold(closing, |balance, (_, amount)| balance.checked_sub(*amount))
        })
        .collect()
}

/// Splits a bank `charge` (positive) over the `items` (positive) of an
/// order: each split gets its item's amount, and the difference between the
/// charge and the items (shipping, gift cards, promotions, partial
/// shipments) goes to the largest item, the first on a tie, so the splits
/// add up to the charge exactly. `None` when there are no items, a currency
/// differs, the sums overflow, or a split would not stay positive.
pub fn split_by_item(charge: Money, items: &[Money]) -> Option<Vec<Money>> {
    let currency = charge.currency();
    if items.iter().any(|item| item.currency() != currency) {
        return None;
    }
    let total = items
        .iter()
        .try_fold(Money::zero(currency), |sum, item| sum.checked_add(*item))
        .ok()?;
    let difference = charge.checked_sub(total).ok()?;

    // `max_by_key` returns the *last* maximum; reversing first makes it the
    // first, so ties go to the earliest item.
    let largest = items
        .iter()
        .enumerate()
        .rev()
        .max_by_key(|(_, item)| item.minor())
        .map(|(index, _)| index)?;
    let mut split = items.to_vec();
    split[largest] = split[largest].checked_add(difference).ok()?;
    split.iter().all(Money::is_positive).then_some(split)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{date, eur, usd};
    use proptest::prelude::*;

    #[test]
    fn a_complete_statement_has_no_gap() {
        let lines = [eur(-2_000), eur(150_000), eur(-500)];
        assert_eq!(
            statement_gap(eur(10_000), &lines, eur(157_500)).unwrap(),
            None
        );
    }

    #[test]
    fn a_missing_line_shows_as_a_gap() {
        let lines = [eur(-2_000), eur(150_000)];
        assert_eq!(
            statement_gap(eur(10_000), &lines, eur(157_500)).unwrap(),
            Some(eur(500))
        );
    }

    #[test]
    fn a_gap_in_mixed_currencies_is_an_error() {
        assert!(statement_gap(eur(0), &[usd(1)], eur(1)).is_err());
    }

    #[test]
    fn balances_walk_back_from_the_closing_balance() {
        let lines = [
            (date(5), eur(-2_000)),
            (date(15), eur(150_000)),
            (date(30), eur(-500)),
        ];
        let balances = balances_at(eur(157_500), &lines, &[date(4), date(15), date(30)]).unwrap();
        assert_eq!(balances, vec![eur(10_000), eur(158_000), eur(157_500)]);
    }

    #[test]
    fn items_take_the_difference_on_the_largest() {
        let split = split_by_item(eur(4_000), &[eur(1_500), eur(2_500), eur(500)]).unwrap();
        assert_eq!(split, vec![eur(1_500), eur(2_000), eur(500)]);
        let shipping = split_by_item(eur(4_999), &[eur(2_000), eur(2_000)]).unwrap();
        assert_eq!(shipping, vec![eur(2_999), eur(2_000)]);
    }

    #[test]
    fn a_feasible_discount_comes_off_the_largest_item() {
        let split = split_by_item(eur(3_000), &[eur(1_500), eur(2_000)]).unwrap();
        assert_eq!(split, vec![eur(1_500), eur(1_500)]);
    }

    #[test]
    fn a_free_item_cannot_have_a_split() {
        // A zero-priced item (a promotion) would be a zero split, which the
        // ledger refuses; the caller falls back to one split.
        assert_eq!(split_by_item(eur(1_000), &[eur(1_000), eur(0)]), None);
    }

    #[test]
    fn an_overpaid_statement_has_a_negative_gap() {
        assert_eq!(
            statement_gap(eur(1_000), &[eur(-200)], eur(900)).unwrap(),
            Some(eur(-100))
        );
    }

    #[test]
    fn balances_ignore_line_order_and_handle_no_dates() {
        let lines = [(date(30), eur(-500)), (date(5), eur(-2_000))];
        assert_eq!(
            balances_at(eur(7_500), &lines, &[date(10)]).unwrap(),
            vec![eur(8_000)]
        );
        assert!(balances_at(eur(7_500), &lines, &[]).unwrap().is_empty());
        assert!(balances_at(eur(0), &[(date(30), usd(1))], &[date(1)]).is_err());
    }

    #[test]
    fn impossible_splits_are_none() {
        assert_eq!(split_by_item(eur(1_000), &[]), None);
        assert_eq!(split_by_item(eur(1_000), &[eur(500), usd(500)]), None);
        // A gift card covered more than the largest item.
        assert_eq!(split_by_item(eur(100), &[eur(500), eur(400)]), None);
        assert_eq!(split_by_item(eur(0), &[eur(500)]), None);
    }

    proptest! {
        #[test]
        fn a_split_always_adds_up_to_the_charge_with_positive_parts(
            charge in 1i64..1_000_000,
            items in proptest::collection::vec(1i64..100_000, 1..8),
        ) {
            let items: Vec<Money> = items.into_iter().map(eur).collect();
            if let Some(split) = split_by_item(eur(charge), &items) {
                prop_assert_eq!(split.len(), items.len());
                prop_assert!(split.iter().all(Money::is_positive));
                prop_assert_eq!(split.iter().map(Money::minor).sum::<i64>(), charge);
            }
        }
    }
}
