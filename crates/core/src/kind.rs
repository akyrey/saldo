//! Split kind: derived from the account types, never chosen by the user.

use serde::{Deserialize, Serialize};

use crate::account::AccountType;
use crate::error::InvalidAccountPair;

/// What a split means, derived from its two accounts (ARCHITECTURE.md 4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitKind {
    Withdrawal,
    Deposit,
    Transfer,
    OpeningBalance,
    Reconciliation,
}

impl SplitKind {
    /// Derives the kind of a split that moves money from `source` to
    /// `destination`, or says the pair is not allowed.
    pub fn derive(
        source: AccountType,
        destination: AccountType,
    ) -> Result<SplitKind, InvalidAccountPair> {
        use crate::account::EquityRole::{OpeningBalance, Reconciliation};
        use AccountType::{Asset, Equity, Expense, Liability, Revenue};

        // `match` on a pair of enums: the compiler checks every combination is
        // covered, here by the final `_` arm. `A | B` matches either case.
        match (source, destination) {
            (Asset | Liability, Expense) => Ok(SplitKind::Withdrawal),
            (Revenue, Asset | Liability) => Ok(SplitKind::Deposit),
            (Asset | Liability, Asset | Liability) => Ok(SplitKind::Transfer),
            (Equity(OpeningBalance), Asset | Liability)
            | (Asset | Liability, Equity(OpeningBalance)) => Ok(SplitKind::OpeningBalance),
            (Equity(Reconciliation), Asset | Liability)
            | (Asset | Liability, Equity(Reconciliation)) => Ok(SplitKind::Reconciliation),
            _ => Err(InvalidAccountPair {
                source,
                destination,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::EquityRole;
    use AccountType::{Asset, Expense, Liability, Revenue};

    const OPENING: AccountType = AccountType::Equity(EquityRole::OpeningBalance);
    const RECONCILIATION: AccountType = AccountType::Equity(EquityRole::Reconciliation);
    const ALL_TYPES: [AccountType; 6] =
        [Asset, Expense, Revenue, Liability, OPENING, RECONCILIATION];

    fn kind(source: AccountType, destination: AccountType) -> Option<SplitKind> {
        SplitKind::derive(source, destination).ok()
    }

    #[test]
    fn withdrawal_goes_from_asset_or_liability_to_expense() {
        assert_eq!(kind(Asset, Expense), Some(SplitKind::Withdrawal));
        assert_eq!(kind(Liability, Expense), Some(SplitKind::Withdrawal));
    }

    #[test]
    fn deposit_goes_from_revenue_to_asset_or_liability() {
        assert_eq!(kind(Revenue, Asset), Some(SplitKind::Deposit));
        assert_eq!(kind(Revenue, Liability), Some(SplitKind::Deposit));
    }

    #[test]
    fn transfer_is_between_assets_and_liabilities() {
        for source in [Asset, Liability] {
            for destination in [Asset, Liability] {
                assert_eq!(kind(source, destination), Some(SplitKind::Transfer));
            }
        }
    }

    #[test]
    fn opening_balance_goes_either_way() {
        for side in [Asset, Liability] {
            assert_eq!(kind(OPENING, side), Some(SplitKind::OpeningBalance));
            assert_eq!(kind(side, OPENING), Some(SplitKind::OpeningBalance));
        }
    }

    #[test]
    fn reconciliation_goes_either_way() {
        for side in [Asset, Liability] {
            assert_eq!(kind(RECONCILIATION, side), Some(SplitKind::Reconciliation));
            assert_eq!(kind(side, RECONCILIATION), Some(SplitKind::Reconciliation));
        }
    }

    #[test]
    fn anything_else_is_an_invalid_pair() {
        let err = SplitKind::derive(Expense, Asset).unwrap_err();
        assert_eq!(
            err,
            InvalidAccountPair {
                source: Expense,
                destination: Asset
            }
        );
        assert_eq!(
            err.to_string(),
            "money cannot move from an account of type expense to an account of type asset"
        );
    }

    /// Every one of the 36 type pairs, against the table in ARCHITECTURE.md 4.1.
    #[test]
    fn exactly_sixteen_of_thirty_six_pairs_are_valid() {
        let valid = [
            (Asset, Expense, SplitKind::Withdrawal),
            (Liability, Expense, SplitKind::Withdrawal),
            (Revenue, Asset, SplitKind::Deposit),
            (Revenue, Liability, SplitKind::Deposit),
            (Asset, Asset, SplitKind::Transfer),
            (Asset, Liability, SplitKind::Transfer),
            (Liability, Asset, SplitKind::Transfer),
            (Liability, Liability, SplitKind::Transfer),
            (OPENING, Asset, SplitKind::OpeningBalance),
            (OPENING, Liability, SplitKind::OpeningBalance),
            (Asset, OPENING, SplitKind::OpeningBalance),
            (Liability, OPENING, SplitKind::OpeningBalance),
            (RECONCILIATION, Asset, SplitKind::Reconciliation),
            (RECONCILIATION, Liability, SplitKind::Reconciliation),
            (Asset, RECONCILIATION, SplitKind::Reconciliation),
            (Liability, RECONCILIATION, SplitKind::Reconciliation),
        ];
        let mut checked = 0;
        for source in ALL_TYPES {
            for destination in ALL_TYPES {
                let expected = valid
                    .iter()
                    .find(|(s, d, _)| *s == source && *d == destination)
                    .map(|(_, _, kind)| *kind);
                assert_eq!(
                    kind(source, destination),
                    expected,
                    "{source} -> {destination}"
                );
                checked += 1;
            }
        }
        assert_eq!(checked, 36);
    }

    #[test]
    fn serde_names_match_the_sql_values() {
        let json = serde_json::to_string(&SplitKind::OpeningBalance).unwrap();
        assert_eq!(json, r#""opening_balance""#);
        let parsed: SplitKind = serde_json::from_str(r#""withdrawal""#).unwrap();
        assert_eq!(parsed, SplitKind::Withdrawal);
    }
}
