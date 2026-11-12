//! Balance changes caused by a transaction group.
//!
//! Balance of an account = what flowed in as destination − what flowed out as
//! source. Every split moves money between two accounts, so the changes of
//! one group always add up to zero per currency (double entry by construction).

use std::collections::BTreeMap;

use crate::error::MoneyError;
use crate::ids::AccountId;
use crate::lookup::AccountLookup;
use crate::money::Money;
use crate::transaction::{NewSplit, ValidatedGroup};

/// The change to each account's balance, two entries per split: the source
/// loses the amount and the destination gains it.
///
/// An account whose currency is the split's foreign currency moves by the
/// foreign amount instead (the USD side of a EUR → USD transfer). Pass the
/// same lookup the group was validated with; an account missing from it is
/// treated as being in the split's own currency.
pub fn balance_deltas(group: &ValidatedGroup, accounts: &AccountLookup) -> Vec<(AccountId, Money)> {
    group
        .splits()
        .iter()
        .flat_map(|validated| {
            let split = validated.split();
            [
                (
                    split.source,
                    moved_for(split, split.source, accounts).negated(),
                ),
                (
                    split.destination,
                    moved_for(split, split.destination, accounts),
                ),
            ]
        })
        .collect()
}

/// How much `account` sees move: the foreign amount when the account is in
/// the foreign currency, otherwise the split amount.
fn moved_for(split: &NewSplit, account: AccountId, accounts: &AccountLookup) -> Money {
    match (split.foreign_amount, accounts.account(account)) {
        (Some(foreign), Some(found)) if found.currency == foreign.currency() => foreign,
        _ => split.amount,
    }
}

/// Adds `deltas` to `balances` and returns the new balances.
///
/// `balances` is taken by value and handed back updated, so the caller's
/// original map is never changed behind its back. Errors if an account's
/// balance and delta are in different currencies, or on overflow.
pub fn apply_deltas(
    balances: BTreeMap<AccountId, Money>,
    deltas: &[(AccountId, Money)],
) -> Result<BTreeMap<AccountId, Money>, MoneyError> {
    // `try_fold` is a loop with an accumulator that stops at the first `Err`.
    // The map is owned by the fold, so updating it in place is safe.
    deltas
        .iter()
        .try_fold(balances, |mut totals, (account, delta)| {
            let current = totals
                .get(account)
                .copied()
                .unwrap_or_else(|| Money::zero(delta.currency()));
            totals.insert(*account, current.checked_add(*delta)?);
            Ok(totals)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::SplitKind;
    use crate::test_support::*;
    use crate::transaction::NewTransactionGroup;
    use crate::validation::validate_group;
    use proptest::prelude::*;

    fn validated(splits: Vec<NewSplit>) -> ValidatedGroup {
        let input = NewTransactionGroup {
            title: None,
            splits,
        };
        validate_group(input, &lookup()).unwrap()
    }

    fn balances_of(splits: Vec<NewSplit>) -> BTreeMap<AccountId, Money> {
        let deltas = balance_deltas(&validated(splits), &lookup());
        apply_deltas(BTreeMap::new(), &deltas).unwrap()
    }

    #[test]
    fn withdrawal_lowers_the_asset_and_raises_the_expense() {
        let deltas = balance_deltas(
            &validated(vec![split(CHECKING, GROCERIES, eur(4250))]),
            &lookup(),
        );
        assert_eq!(
            deltas,
            vec![(id(CHECKING), eur(-4250)), (id(GROCERIES), eur(4250))]
        );
    }

    #[test]
    fn foreign_currency_transfer_moves_the_foreign_amount_into_the_destination() {
        let mut transfer = split(CHECKING, USD_WALLET, eur(10000));
        transfer.foreign_amount = Some(usd(10800));
        let balances = balances_of(vec![transfer]);
        assert_eq!(balances[&id(CHECKING)], eur(-10000));
        assert_eq!(balances[&id(USD_WALLET)], usd(10800));
    }

    #[test]
    fn foreign_purchase_moves_the_amount_on_both_sides() {
        let mut purchase = split(CHECKING, GROCERIES, eur(920));
        purchase.foreign_amount = Some(usd(1000));
        let balances = balances_of(vec![purchase]);
        assert_eq!(balances[&id(CHECKING)], eur(-920));
        assert_eq!(balances[&id(GROCERIES)], eur(920));
    }

    /// The Phase 1 headline: three splits with different sources,
    /// destinations and kinds, and the resulting balance of every account.
    #[test]
    fn three_split_group_balances() {
        let group = validated(vec![
            split(CHECKING, GROCERIES, eur(4250)),
            split(SALARY, CARD, eur(1999)),
            split(CARD, SAVINGS, eur(10000)),
        ]);
        let kinds: Vec<_> = group.splits().iter().map(|s| s.kind()).collect();
        assert_eq!(
            kinds,
            vec![
                SplitKind::Withdrawal,
                SplitKind::Deposit,
                SplitKind::Transfer
            ]
        );

        let deltas = balance_deltas(&group, &lookup());
        let balances = apply_deltas(BTreeMap::new(), &deltas).unwrap();

        assert_eq!(balances.len(), 5);
        assert_eq!(balances[&id(CHECKING)], eur(-4250));
        assert_eq!(balances[&id(GROCERIES)], eur(4250));
        assert_eq!(balances[&id(SALARY)], eur(-1999));
        assert_eq!(balances[&id(CARD)], eur(1999 - 10000));
        assert_eq!(balances[&id(SAVINGS)], eur(10000));
    }

    #[test]
    fn apply_adds_to_existing_balances_without_touching_the_input() {
        let opening = BTreeMap::from([(id(CHECKING), eur(100000))]);
        let deltas = [(id(CHECKING), eur(-2500)), (id(GROCERIES), eur(2500))];

        let updated = apply_deltas(opening.clone(), &deltas).unwrap();

        assert_eq!(updated[&id(CHECKING)], eur(97500));
        assert_eq!(updated[&id(GROCERIES)], eur(2500));
        assert_eq!(opening[&id(CHECKING)], eur(100000));
    }

    #[test]
    fn apply_rejects_mixed_currencies_and_overflow() {
        let balances = BTreeMap::from([(id(CHECKING), eur(100))]);
        let mixed = apply_deltas(balances, &[(id(CHECKING), usd(1))]);
        assert!(matches!(mixed, Err(MoneyError::CurrencyMismatch { .. })));

        let full = BTreeMap::from([(id(CHECKING), eur(i64::MAX))]);
        let overflow = apply_deltas(full, &[(id(CHECKING), eur(1))]);
        assert_eq!(overflow, Err(MoneyError::Overflow));
    }

    /// Every valid pair among the EUR accounts.
    fn eur_pairs() -> Vec<(u128, u128)> {
        let eur_accounts = [
            CHECKING,
            SAVINGS,
            CARD,
            GROCERIES,
            SALARY,
            OPENING,
            RECONCILIATION,
        ];
        let all = accounts();
        let type_of = |n: u128| all.iter().find(|a| a.id == id(n)).map(|a| a.account_type);
        let mut pairs = Vec::new();
        for source in eur_accounts {
            for destination in eur_accounts {
                let (Some(s), Some(d)) = (type_of(source), type_of(destination)) else {
                    continue;
                };
                if source != destination && SplitKind::derive(s, d).is_ok() {
                    pairs.push((source, destination));
                }
            }
        }
        pairs
    }

    proptest! {
        #[test]
        fn single_currency_deltas_sum_to_zero(
            picks in prop::collection::vec(
                (prop::sample::select(eur_pairs()), 1i64..=1_000_000_000_000),
                1..10,
            )
        ) {
            let splits = picks
                .into_iter()
                .map(|((source, destination), minor)| split(source, destination, eur(minor)))
                .collect();
            let deltas = balance_deltas(&validated(splits), &lookup());
            let total: i128 = deltas.iter().map(|(_, m)| i128::from(m.minor())).sum();
            prop_assert_eq!(total, 0);
        }
    }
}
