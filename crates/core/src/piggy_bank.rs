//! Piggy banks: money set aside inside asset accounts (ARCHITECTURE.md 5.3).
//!
//! The saved amount is the sum of the piggy bank's events. Adding money may
//! not push the amount saved in an account above that account's balance,
//! nor the total above the target; removing may not go below zero.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::error::IdError;

/// Identifies a piggy bank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PiggyBankId(pub Uuid);

impl fmt::Display for PiggyBankId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for PiggyBankId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(Self).map_err(|_| IdError {
            input: s.to_string(),
        })
    }
}

/// What a new event is checked against, all in minor units of the piggy
/// bank's currency. The caller loads these from the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PiggyBankState {
    /// Sum of every event so far.
    pub saved_total: i64,
    /// Sum of the events booked on the event's account.
    pub saved_in_account: i64,
    /// The account's current balance.
    pub account_balance: i64,
    pub target: Option<i64>,
}

/// The result of an accepted event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventOutcome {
    pub saved_total: i64,
    /// True when this event crossed the target (from below to at or above).
    pub target_reached: bool,
}

/// Why an event is refused. `code()` is the stable string the API reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PiggyEventError {
    #[error("the amount must not be zero")]
    ZeroAmount,
    #[error("only {saved} minor units are saved in this account")]
    BelowZero { saved: i64 },
    #[error("only {room} minor units are left until the target")]
    ExceedsTarget { room: i64 },
    #[error("only {available} minor units of the account balance are not yet saved")]
    ExceedsBalance { available: i64 },
    #[error("the amount is too large")]
    Overflow,
}

impl PiggyEventError {
    pub fn code(&self) -> &'static str {
        match self {
            PiggyEventError::ZeroAmount => "amount_zero",
            PiggyEventError::BelowZero { .. } => "piggy_bank_below_zero",
            PiggyEventError::ExceedsTarget { .. } => "piggy_bank_exceeds_target",
            PiggyEventError::ExceedsBalance { .. } => "piggy_bank_exceeds_balance",
            PiggyEventError::Overflow => "amount_overflow",
        }
    }
}

/// Checks an event of `amount` (positive adds, negative removes) against
/// every rule.
pub fn check_event(state: PiggyBankState, amount: i64) -> Result<EventOutcome, PiggyEventError> {
    let outcome = check_imported_event(state, amount)?;
    let in_account = state
        .saved_in_account
        .checked_add(amount)
        .ok_or(PiggyEventError::Overflow)?;
    if in_account < 0 {
        return Err(PiggyEventError::BelowZero {
            saved: state.saved_in_account,
        });
    }
    if amount > 0 {
        if let Some(target) = state.target
            && outcome.saved_total > target
        {
            return Err(PiggyEventError::ExceedsTarget {
                room: (target - state.saved_total).max(0),
            });
        }
        if in_account > state.account_balance {
            return Err(PiggyEventError::ExceedsBalance {
                available: (state.account_balance - state.saved_in_account).max(0),
            });
        }
    }
    Ok(outcome)
}

/// The checks that still apply to imported history, which may break the
/// balance and target rules (it was recorded elsewhere, at another time).
pub fn check_imported_event(
    state: PiggyBankState,
    amount: i64,
) -> Result<EventOutcome, PiggyEventError> {
    if amount == 0 {
        return Err(PiggyEventError::ZeroAmount);
    }
    let saved_total = state
        .saved_total
        .checked_add(amount)
        .ok_or(PiggyEventError::Overflow)?;
    let target_reached = state
        .target
        .is_some_and(|target| state.saved_total < target && saved_total >= target);
    Ok(EventOutcome {
        saved_total,
        target_reached,
    })
}

/// The largest amount that can be added now (zero if none): what is left
/// until the target, and of the account balance not yet saved.
pub fn room_to_add(state: PiggyBankState) -> i64 {
    let by_balance = state.account_balance.saturating_sub(state.saved_in_account);
    let by_target = state
        .target
        .map_or(i64::MAX, |target| target.saturating_sub(state.saved_total));
    by_balance.min(by_target).max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> PiggyBankState {
        PiggyBankState {
            saved_total: 300,
            saved_in_account: 200,
            account_balance: 1_000,
            target: Some(1_000),
        }
    }

    #[test]
    fn adding_and_removing_within_the_limits_is_accepted() {
        assert_eq!(
            check_event(state(), 100),
            Ok(EventOutcome {
                saved_total: 400,
                target_reached: false
            })
        );
        assert_eq!(check_event(state(), -200).unwrap().saved_total, 100);
    }

    #[test]
    fn zero_is_refused() {
        assert_eq!(check_event(state(), 0), Err(PiggyEventError::ZeroAmount));
    }

    #[test]
    fn removing_more_than_the_account_holds_is_refused() {
        assert_eq!(
            check_event(state(), -201),
            Err(PiggyEventError::BelowZero { saved: 200 })
        );
    }

    #[test]
    fn going_past_the_target_is_refused() {
        assert_eq!(
            check_event(state(), 701),
            Err(PiggyEventError::ExceedsTarget { room: 700 })
        );
    }

    #[test]
    fn saving_more_than_the_account_balance_is_refused() {
        let poor = PiggyBankState {
            account_balance: 250,
            ..state()
        };
        assert_eq!(
            check_event(poor, 51),
            Err(PiggyEventError::ExceedsBalance { available: 50 })
        );
        assert!(check_event(poor, 50).is_ok());
        // Removing is always fine while something is saved, even when the
        // balance has dropped below the saved amount since.
        let overdrawn = PiggyBankState {
            account_balance: -10,
            ..state()
        };
        assert!(check_event(overdrawn, -10).is_ok());
    }

    #[test]
    fn reaching_the_target_is_reported_once() {
        let outcome = check_event(state(), 700).unwrap();
        assert!(outcome.target_reached);
        let full = PiggyBankState {
            saved_total: 1_000,
            saved_in_account: 900,
            ..state()
        };
        assert!(!check_event(full, -1).unwrap().target_reached);
        let no_target = PiggyBankState {
            target: None,
            ..state()
        };
        assert!(!check_event(no_target, 700).unwrap().target_reached);
    }

    #[test]
    fn imported_events_skip_balance_and_target_checks() {
        let outcome = check_imported_event(state(), 5_000).unwrap();
        assert_eq!(outcome.saved_total, 5_300);
        assert!(outcome.target_reached);
        assert!(check_imported_event(state(), -1_000).is_ok());
        assert_eq!(
            check_imported_event(state(), 0),
            Err(PiggyEventError::ZeroAmount)
        );
    }

    #[test]
    fn overflow_is_an_error_not_a_panic() {
        let big = PiggyBankState {
            saved_total: i64::MAX,
            saved_in_account: i64::MAX,
            account_balance: i64::MAX,
            target: None,
        };
        assert_eq!(check_event(big, 1), Err(PiggyEventError::Overflow));
        let in_account = PiggyBankState {
            saved_total: 0,
            ..big
        };
        assert_eq!(check_event(in_account, 1), Err(PiggyEventError::Overflow));
    }

    #[test]
    fn room_to_add_is_the_smaller_limit_and_never_negative() {
        assert_eq!(room_to_add(state()), 700);
        let poor = PiggyBankState {
            account_balance: 250,
            ..state()
        };
        assert_eq!(room_to_add(poor), 50);
        let overdrawn = PiggyBankState {
            account_balance: 0,
            ..state()
        };
        assert_eq!(room_to_add(overdrawn), 0);
    }

    #[test]
    fn codes_are_stable_and_messages_readable() {
        let errors = [
            PiggyEventError::ZeroAmount,
            PiggyEventError::BelowZero { saved: 1 },
            PiggyEventError::ExceedsTarget { room: 1 },
            PiggyEventError::ExceedsBalance { available: 1 },
            PiggyEventError::Overflow,
        ];
        let codes: Vec<_> = errors.iter().map(PiggyEventError::code).collect();
        assert_eq!(
            codes,
            [
                "amount_zero",
                "piggy_bank_below_zero",
                "piggy_bank_exceeds_target",
                "piggy_bank_exceeds_balance",
                "amount_overflow"
            ]
        );
        assert!(errors.iter().all(|e| !e.to_string().is_empty()));
    }

    #[test]
    fn ids_parse_and_display() {
        let text = "0190f5c3-7a2b-7c00-8000-000000000002";
        assert_eq!(text.parse::<PiggyBankId>().unwrap().to_string(), text);
        assert!("x".parse::<PiggyBankId>().is_err());
    }
}
