//! Validation of transaction groups against the ledger rules (ARCHITECTURE.md 4.2).

use crate::account::Account;
use crate::error::{Reference, ValidationError};
use crate::ids::AccountId;
use crate::kind::SplitKind;
use crate::lookup::AccountLookup;
use crate::transaction::{NewSplit, NewTransactionGroup, ValidatedGroup};

/// Checks every ledger rule and returns either a [`ValidatedGroup`] or every
/// error found, each tagged with the index of its split.
///
/// `input` is taken by value: the group is *moved* into this function and the
/// caller cannot use it afterwards. In PHP or Go you could keep using it; Rust
/// treats it like handing over the only copy of a document. That lets the
/// validated group reuse the same splits without copying them.
pub fn validate_group(
    input: NewTransactionGroup,
    accounts: &AccountLookup,
) -> Result<ValidatedGroup, Vec<ValidationError>> {
    if input.splits.is_empty() {
        return Err(vec![ValidationError::EmptyGroup]);
    }

    let mut errors = Vec::new();
    let mut kinds = Vec::new();
    for (index, split) in input.splits.iter().enumerate() {
        match validate_split(index, split, accounts) {
            Ok(kind) => kinds.push(kind),
            Err(split_errors) => errors.extend(split_errors),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    // Every split is valid: pair each one with its kind. `into_iter` consumes
    // the vector, moving the splits instead of copying them.
    let splits = input.splits.into_iter().zip(kinds).collect();
    Ok(ValidatedGroup::new(
        accounts.household_id(),
        input.title,
        splits,
    ))
}

/// All checks for one split; the kind if it passes.
fn validate_split(
    index: usize,
    split: &NewSplit,
    lookup: &AccountLookup,
) -> Result<SplitKind, Vec<ValidationError>> {
    let mut errors = check_amounts(index, split);
    if split.source == split.destination {
        errors.push(ValidationError::SameAccount { split: index });
    }
    errors.extend(check_references(index, split, lookup));

    match check_accounts(index, split, lookup) {
        Ok(kind) if errors.is_empty() => Ok(kind),
        Ok(_) => Err(errors),
        Err(account_errors) => {
            errors.extend(account_errors);
            Err(errors)
        }
    }
}

/// Invariants 2 and the first half of 7: amounts are positive and a foreign
/// amount is in another currency.
fn check_amounts(index: usize, split: &NewSplit) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    if !split.amount.is_positive() {
        errors.push(ValidationError::AmountNotPositive { split: index });
    }
    // `if let` runs the block only when the `Option` holds a value.
    if let Some(foreign) = split.foreign_amount {
        if !foreign.is_positive() {
            errors.push(ValidationError::ForeignAmountNotPositive { split: index });
        }
        if foreign.currency() == split.amount.currency() {
            errors.push(ValidationError::ForeignCurrencySameAsAmount { split: index });
        }
    }
    errors
}

/// Invariant 5 for everything except accounts.
fn check_references(
    index: usize,
    split: &NewSplit,
    lookup: &AccountLookup,
) -> Vec<ValidationError> {
    let mut missing = Vec::new();
    if let Some(id) = split.category.filter(|id| !lookup.has_category(*id)) {
        missing.push(Reference::Category(id));
    }
    if let Some(id) = split.budget.filter(|id| !lookup.has_budget(*id)) {
        missing.push(Reference::Budget(id));
    }
    if let Some(id) = split.bill.filter(|id| !lookup.has_bill(*id)) {
        missing.push(Reference::Bill(id));
    }
    for id in split.tags.iter().filter(|id| !lookup.has_tag(**id)) {
        missing.push(Reference::Tag(*id));
    }
    missing
        .into_iter()
        .map(|reference| ValidationError::UnknownReference {
            split: index,
            reference,
        })
        .collect()
}

/// Invariants 4, 5 (accounts), 6, 7 (transfers) and 8. Returns the kind.
fn check_accounts(
    index: usize,
    split: &NewSplit,
    lookup: &AccountLookup,
) -> Result<SplitKind, Vec<ValidationError>> {
    let (source, destination) = match (
        resolve(index, split.source, lookup),
        resolve(index, split.destination, lookup),
    ) {
        (Ok(source), Ok(destination)) => (source, destination),
        (source, destination) => {
            // Keep whichever lookups failed. `.err()` turns a `Result` into an
            // `Option` of its error, and `flatten` drops the `None`s.
            return Err([source.err(), destination.err()]
                .into_iter()
                .flatten()
                .collect());
        }
    };

    let kind =
        SplitKind::derive(source.account_type, destination.account_type).map_err(|pair| {
            vec![ValidationError::InvalidAccountPair {
                split: index,
                source_type: pair.source,
                destination_type: pair.destination,
            }]
        })?;

    let mut errors = check_active(index, source, destination);
    errors.extend(check_currencies(index, split, kind, source, destination));
    if errors.is_empty() {
        Ok(kind)
    } else {
        Err(errors)
    }
}

/// Finds an account and checks it belongs to the lookup's household.
///
/// The returned `&Account` borrows from `lookup`; the compiler makes sure it
/// is not used after the lookup is gone.
fn resolve(
    index: usize,
    id: AccountId,
    lookup: &AccountLookup,
) -> Result<&Account, ValidationError> {
    let account = lookup.account(id).ok_or(ValidationError::UnknownAccount {
        split: index,
        account: id,
    })?;
    if account.household_id != lookup.household_id() {
        return Err(ValidationError::AccountOutsideHousehold {
            split: index,
            account: id,
        });
    }
    Ok(account)
}

/// Invariant 8. The same account is reported once even if it is both sides.
fn check_active(index: usize, source: &Account, destination: &Account) -> Vec<ValidationError> {
    let mut sides = vec![source];
    if destination.id != source.id {
        sides.push(destination);
    }
    sides
        .into_iter()
        .filter(|account| !account.active)
        .map(|account| ValidationError::AccountInactive {
            split: index,
            account: account.id,
        })
        .collect()
}

/// Invariant 6 and the transfer half of 7.
fn check_currencies(
    index: usize,
    split: &NewSplit,
    kind: SplitKind,
    source: &Account,
    destination: &Account,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    // For every valid kind, the currency comes from the asset/liability side,
    // and for a transfer (both sides qualify) from the source.
    let home = if source.account_type.is_asset_or_liability() {
        source
    } else {
        destination
    };
    if split.amount.currency() != home.currency {
        errors.push(ValidationError::CurrencyMismatch {
            split: index,
            expected: home.currency,
            found: split.amount.currency(),
        });
    }

    let cross_currency = kind == SplitKind::Transfer && source.currency != destination.currency;
    if cross_currency {
        let expected = destination.currency;
        match split.foreign_amount {
            None => errors.push(ValidationError::ForeignAmountRequired {
                split: index,
                expected,
            }),
            Some(foreign) if foreign.currency() != expected => {
                errors.push(ValidationError::ForeignCurrencyMismatch {
                    split: index,
                    expected,
                    found: foreign.currency(),
                })
            }
            Some(_) => {}
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{AccountType, EquityRole};
    use crate::error::Reference;
    use crate::ids::{BillId, BudgetId, CategoryId, TagId};
    use crate::kind::SplitKind;
    use crate::money::Money;
    use crate::test_support::*;
    use crate::transaction::NewSplit;
    use uuid::Uuid;

    fn group(splits: Vec<NewSplit>) -> NewTransactionGroup {
        NewTransactionGroup {
            title: Some("test group".to_string()),
            splits,
        }
    }

    fn errors_of(splits: Vec<NewSplit>) -> Vec<ValidationError> {
        validate_group(group(splits), &lookup()).unwrap_err()
    }

    fn kinds_of(splits: Vec<NewSplit>) -> Vec<SplitKind> {
        let validated = validate_group(group(splits), &lookup()).unwrap();
        validated.splits().iter().map(|s| s.kind()).collect()
    }

    // Invariant 1: at least one split.
    #[test]
    fn empty_group_is_rejected() {
        assert_eq!(errors_of(vec![]), vec![ValidationError::EmptyGroup]);
    }

    #[test]
    fn single_split_group_is_accepted() {
        let validated =
            validate_group(group(vec![split(CHECKING, GROCERIES, eur(500))]), &lookup()).unwrap();
        assert_eq!(validated.household_id(), household());
        assert_eq!(validated.title(), Some("test group"));
        assert_eq!(validated.splits().len(), 1);
        assert_eq!(validated.splits()[0].split().amount, eur(500));
    }

    // Invariant 2: amount > 0.
    #[test]
    fn zero_or_negative_amount_is_rejected() {
        for minor in [0, -1] {
            assert_eq!(
                errors_of(vec![split(CHECKING, GROCERIES, eur(minor))]),
                vec![ValidationError::AmountNotPositive { split: 0 }]
            );
        }
    }

    // Invariant 3: source != destination.
    #[test]
    fn same_account_is_rejected() {
        assert_eq!(
            errors_of(vec![split(CHECKING, CHECKING, eur(100))]),
            vec![ValidationError::SameAccount { split: 0 }]
        );
    }

    #[test]
    fn different_accounts_of_the_same_type_are_a_transfer() {
        assert_eq!(
            kinds_of(vec![split(CHECKING, SAVINGS, eur(100))]),
            vec![SplitKind::Transfer]
        );
    }

    // Invariant 4: the account pair gives a valid kind.
    #[test]
    fn invalid_account_pair_is_rejected() {
        assert_eq!(
            errors_of(vec![split(GROCERIES, CHECKING, eur(100))]),
            vec![ValidationError::InvalidAccountPair {
                split: 0,
                source_type: AccountType::Expense,
                destination_type: AccountType::Asset,
            }]
        );
    }

    #[test]
    fn every_valid_kind_is_derived() {
        let splits = vec![
            split(CHECKING, GROCERIES, eur(1)),
            split(SALARY, CHECKING, eur(1)),
            split(CHECKING, CARD, eur(1)),
            split(OPENING, CHECKING, eur(1)),
            split(CHECKING, RECONCILIATION, eur(1)),
        ];
        assert_eq!(
            kinds_of(splits),
            vec![
                SplitKind::Withdrawal,
                SplitKind::Deposit,
                SplitKind::Transfer,
                SplitKind::OpeningBalance,
                SplitKind::Reconciliation,
            ]
        );
    }

    // Invariant 5: everything belongs to the group's household.
    #[test]
    fn unknown_accounts_are_rejected() {
        assert_eq!(
            errors_of(vec![split(MISSING, GROCERIES, eur(100))]),
            vec![ValidationError::UnknownAccount {
                split: 0,
                account: id(MISSING)
            }]
        );
        assert_eq!(
            errors_of(vec![split(CHECKING, MISSING, eur(100))]),
            vec![ValidationError::UnknownAccount {
                split: 0,
                account: id(MISSING)
            }]
        );
    }

    #[test]
    fn account_of_another_household_is_rejected() {
        assert_eq!(
            errors_of(vec![split(NEIGHBOUR, GROCERIES, eur(100))]),
            vec![ValidationError::AccountOutsideHousehold {
                split: 0,
                account: id(NEIGHBOUR)
            }]
        );
    }

    #[test]
    fn unknown_category_budget_bill_and_tag_are_rejected() {
        let other = Uuid::from_u128(999);
        let mut bad = split(CHECKING, GROCERIES, eur(100));
        bad.category = Some(CategoryId(other));
        bad.budget = Some(BudgetId(other));
        bad.bill = Some(BillId(other));
        bad.tags = vec![TagId(Uuid::from_u128(TAG)), TagId(other)];
        let unknown = |reference| ValidationError::UnknownReference {
            split: 0,
            reference,
        };
        assert_eq!(
            errors_of(vec![bad]),
            vec![
                unknown(Reference::Category(CategoryId(other))),
                unknown(Reference::Budget(BudgetId(other))),
                unknown(Reference::Bill(BillId(other))),
                unknown(Reference::Tag(TagId(other))),
            ]
        );
    }

    #[test]
    fn known_category_budget_bill_and_tag_are_accepted() {
        let mut good = split(CHECKING, GROCERIES, eur(100));
        good.category = Some(CategoryId(Uuid::from_u128(CATEGORY)));
        good.budget = Some(BudgetId(Uuid::from_u128(BUDGET)));
        good.bill = Some(BillId(Uuid::from_u128(BILL)));
        good.tags = vec![TagId(Uuid::from_u128(TAG))];
        assert_eq!(kinds_of(vec![good]), vec![SplitKind::Withdrawal]);
    }

    // Invariant 6: the split currency follows the asset/liability side.
    #[test]
    fn withdrawal_must_use_the_source_currency() {
        assert_eq!(
            errors_of(vec![split(CHECKING, GROCERIES, usd(100))]),
            vec![ValidationError::CurrencyMismatch {
                split: 0,
                expected: code("EUR"),
                found: code("USD"),
            }]
        );
    }

    #[test]
    fn deposit_must_use_the_destination_currency() {
        assert_eq!(
            errors_of(vec![split(SALARY, USD_WALLET, eur(100))]),
            vec![ValidationError::CurrencyMismatch {
                split: 0,
                expected: code("USD"),
                found: code("EUR"),
            }]
        );
        assert_eq!(
            kinds_of(vec![split(SALARY, USD_WALLET, usd(100))]),
            vec![SplitKind::Deposit]
        );
    }

    #[test]
    fn transfer_must_use_the_source_currency() {
        let mut wrong = split(USD_WALLET, CHECKING, eur(100));
        wrong.foreign_amount = Some(eur(90));
        let errors = errors_of(vec![wrong]);
        assert!(errors.contains(&ValidationError::CurrencyMismatch {
            split: 0,
            expected: code("USD"),
            found: code("EUR"),
        }));
    }

    #[test]
    fn opening_balance_uses_the_asset_side_currency_in_either_direction() {
        // The equity account is in EUR, but the USD wallet decides.
        assert_eq!(
            kinds_of(vec![
                split(OPENING, USD_WALLET, usd(100)),
                split(USD_WALLET, RECONCILIATION, usd(5)),
            ]),
            vec![SplitKind::OpeningBalance, SplitKind::Reconciliation]
        );
        assert_eq!(
            errors_of(vec![split(OPENING, USD_WALLET, eur(100))]),
            vec![ValidationError::CurrencyMismatch {
                split: 0,
                expected: code("USD"),
                found: code("EUR"),
            }]
        );
    }

    // Invariant 7: foreign amounts.
    #[test]
    fn foreign_amount_must_be_positive() {
        let mut bad = split(CHECKING, GROCERIES, eur(100));
        bad.foreign_amount = Some(usd(0));
        assert_eq!(
            errors_of(vec![bad]),
            vec![ValidationError::ForeignAmountNotPositive { split: 0 }]
        );
    }

    #[test]
    fn foreign_amount_must_differ_in_currency() {
        let mut bad = split(CHECKING, GROCERIES, eur(100));
        bad.foreign_amount = Some(eur(100));
        assert_eq!(
            errors_of(vec![bad]),
            vec![ValidationError::ForeignCurrencySameAsAmount { split: 0 }]
        );
    }

    #[test]
    fn foreign_purchase_is_accepted() {
        let mut purchase = split(CHECKING, GROCERIES, eur(920));
        purchase.foreign_amount = Some(usd(1000));
        assert_eq!(kinds_of(vec![purchase]), vec![SplitKind::Withdrawal]);
    }

    #[test]
    fn cross_currency_transfer_needs_a_foreign_amount() {
        assert_eq!(
            errors_of(vec![split(CHECKING, USD_WALLET, eur(100))]),
            vec![ValidationError::ForeignAmountRequired {
                split: 0,
                expected: code("USD"),
            }]
        );
    }

    #[test]
    fn cross_currency_transfer_foreign_amount_is_in_destination_currency() {
        let mut bad = split(CHECKING, USD_WALLET, eur(100));
        bad.foreign_amount = Some(Money::new(80, code("GBP")));
        assert_eq!(
            errors_of(vec![bad]),
            vec![ValidationError::ForeignCurrencyMismatch {
                split: 0,
                expected: code("USD"),
                found: code("GBP"),
            }]
        );

        let mut good = split(CHECKING, USD_WALLET, eur(100));
        good.foreign_amount = Some(usd(108));
        assert_eq!(kinds_of(vec![good]), vec![SplitKind::Transfer]);
    }

    // Invariant 8: both accounts are active.
    #[test]
    fn inactive_account_is_rejected() {
        assert_eq!(
            errors_of(vec![split(ARCHIVED, GROCERIES, eur(100))]),
            vec![ValidationError::AccountInactive {
                split: 0,
                account: id(ARCHIVED)
            }]
        );
        assert_eq!(
            errors_of(vec![split(CHECKING, ARCHIVED, eur(100))]),
            vec![ValidationError::AccountInactive {
                split: 0,
                account: id(ARCHIVED)
            }]
        );
    }

    // Invariant 9, the headline case: three splits with different sources,
    // destinations, kinds and dates in one group.
    #[test]
    fn splits_may_differ_in_accounts_kinds_and_dates() {
        let mut groceries = split(CHECKING, GROCERIES, eur(4250));
        groceries.date = date(1);
        let mut refund = split(SALARY, CARD, eur(1999));
        refund.date = date(2);
        let mut savings = split(CARD, SAVINGS, eur(10000));
        savings.date = date(3);

        let validated = validate_group(group(vec![groceries, refund, savings]), &lookup()).unwrap();

        let kinds: Vec<_> = validated.splits().iter().map(|s| s.kind()).collect();
        assert_eq!(
            kinds,
            vec![
                SplitKind::Withdrawal,
                SplitKind::Deposit,
                SplitKind::Transfer
            ]
        );
        let dates: Vec<_> = validated.splits().iter().map(|s| s.split().date).collect();
        assert_eq!(dates, vec![date(1), date(2), date(3)]);
    }

    #[test]
    fn errors_from_every_split_are_collected_with_their_index() {
        let errors = errors_of(vec![
            split(CHECKING, GROCERIES, eur(100)),
            split(CHECKING, CHECKING, eur(0)),
            split(GROCERIES, SALARY, eur(100)),
        ]);
        assert_eq!(
            errors,
            vec![
                ValidationError::AmountNotPositive { split: 1 },
                ValidationError::SameAccount { split: 1 },
                ValidationError::InvalidAccountPair {
                    split: 2,
                    source_type: AccountType::Expense,
                    destination_type: AccountType::Revenue,
                },
            ]
        );
    }

    #[test]
    fn same_inactive_account_is_reported_once() {
        let errors = errors_of(vec![split(ARCHIVED, ARCHIVED, eur(100))]);
        assert_eq!(
            errors,
            vec![
                ValidationError::SameAccount { split: 0 },
                ValidationError::AccountInactive {
                    split: 0,
                    account: id(ARCHIVED)
                },
            ]
        );
    }

    #[test]
    fn equity_to_equity_is_an_invalid_pair() {
        let errors = errors_of(vec![split(OPENING, RECONCILIATION, eur(100))]);
        assert_eq!(
            errors,
            vec![ValidationError::InvalidAccountPair {
                split: 0,
                source_type: AccountType::Equity(EquityRole::OpeningBalance),
                destination_type: AccountType::Equity(EquityRole::Reconciliation),
            }]
        );
    }

    #[test]
    fn validated_group_serialises_with_the_kind() {
        let validated =
            validate_group(group(vec![split(CHECKING, GROCERIES, eur(500))]), &lookup()).unwrap();
        let json = serde_json::to_value(&validated).unwrap();
        assert_eq!(json["splits"][0]["kind"], "withdrawal");
        assert_eq!(json["splits"][0]["amount"]["minor"], 500);
    }

    #[test]
    fn new_group_round_trips_through_json() {
        let input = group(vec![split(CHECKING, GROCERIES, eur(500))]);
        let json = serde_json::to_string(&input).unwrap();
        assert_eq!(
            serde_json::from_str::<NewTransactionGroup>(&json).unwrap(),
            input
        );
    }
}
