//! Saldo domain: money, accounts, transaction groups, validation and balances.
//!
//! Pure code: no I/O, no async, no SQL, no clock and no random IDs. Anything
//! that comes from the outside world (accounts from the database, new IDs,
//! today's date) is passed in by the caller, so every function here is
//! deterministic and easy to test.
//!
//! # Example
//!
//! Pay for groceries from a checking account, then compute the balances.
//!
//! ```
//! use std::collections::BTreeMap;
//!
//! use chrono::NaiveDate;
//! use saldo_core::{
//!     apply_deltas, balance_deltas, validate_group, Account, AccountId, AccountLookup,
//!     AccountType, Currency, CurrencyCode, HouseholdId, Money, NewSplit,
//!     NewTransactionGroup, SplitKind, SplitMeta,
//! };
//! use uuid::Uuid;
//!
//! // Tests and examples may use expect(); application code returns errors.
//! let eur = Currency::new(CurrencyCode::new("EUR").expect("valid code"), 2).expect("valid places");
//! let household = HouseholdId(Uuid::from_u128(1));
//! let account = |n: u128, name: &str, account_type| Account {
//!     id: AccountId(Uuid::from_u128(n)),
//!     household_id: household,
//!     owner: None,
//!     name: name.to_string(),
//!     account_type,
//!     currency: eur.code(),
//!     active: true,
//! };
//! let checking = account(10, "Checking", AccountType::Asset);
//! let groceries = account(20, "Groceries", AccountType::Expense);
//!
//! // The caller loads the referenced accounts and hands them over.
//! let lookup = AccountLookup::new(household)
//!     .with_account(checking.clone())
//!     .with_account(groceries.clone());
//!
//! let input = NewTransactionGroup {
//!     title: None,
//!     splits: vec![NewSplit {
//!         date: NaiveDate::from_ymd_opt(2026, 11, 13).expect("valid date"),
//!         description: "Weekly shop".to_string(),
//!         source: checking.id,
//!         destination: groceries.id,
//!         amount: Money::parse_decimal("42.50", &eur).expect("valid amount"),
//!         foreign_amount: None,
//!         category: None,
//!         budget: None,
//!         bill: None,
//!         tags: vec![],
//!         reconciled: false,
//!         notes: None,
//!         meta: SplitMeta::default(),
//!     }],
//! };
//!
//! let group = validate_group(input, &lookup).expect("the group is valid");
//! assert_eq!(group.splits()[0].kind(), SplitKind::Withdrawal);
//!
//! let deltas = balance_deltas(&group, &lookup);
//! let balances = apply_deltas(BTreeMap::new(), &deltas).expect("no overflow");
//! assert_eq!(balances[&checking.id].to_decimal_string(&eur), "-42.50");
//! assert_eq!(balances[&groceries.id].to_decimal_string(&eur), "42.50");
//! ```

// `mod x;` pulls in `x.rs` as a module (a namespace, like a PHP namespace).
// `pub` makes it visible outside the crate: Go's capitalised names.
pub mod account;
pub mod alerts;
pub mod balance;
pub mod bills;
pub mod budget;
pub mod carry;
pub mod currency;
pub mod error;
pub mod forecast;
pub mod goal;
pub mod ids;
pub mod kind;
pub mod lookup;
pub mod money;
pub mod period;
pub mod piggy_bank;
pub mod ratios;
pub mod reconcile;
pub mod recurrence;
pub mod rules;
pub mod safe_to_spend;
pub mod subscriptions;
pub mod transaction;
pub mod validation;

#[cfg(test)]
mod test_support;

// Re-exports: callers write `saldo_core::Money` instead of
// `saldo_core::money::Money`, like a Laravel facade or a Go package that
// exposes one flat API.
pub use account::{Account, AccountType, EquityRole};
pub use balance::{apply_deltas, balance_deltas};
pub use bills::{
    Bill, BillFrequency, BillPeriod, BillSchedule, PeriodStatus, ReminderKind, reminder_due,
};
pub use budget::{AutoBudget, AutoBudgetType, Limit, Spend, auto_limits, available, spent};
pub use carry::{Carry, carried};
pub use currency::{Currency, CurrencyCode, MAX_DECIMAL_PLACES};
pub use error::{
    CurrencyError, IdError, InvalidAccountPair, MoneyError, Reference, ValidationError,
};
pub use forecast::{BalancePoint, Planned, Projection, project};
pub use ids::{
    AccountId, AttachmentId, BillId, BillPeriodId, BudgetId, CategoryId, GroupId, HouseholdId,
    SplitId, TagId, UserId,
};
pub use kind::SplitKind;
pub use lookup::AccountLookup;
pub use money::Money;
pub use period::{DateRange, PERIODS, Period};
pub use reconcile::reconcile;
pub use rules::{
    Action, Rule, RuleAction, RuleAmount, RuleError, RuleTrigger, SplitView as RuleSplit, Trigger,
};
pub use subscriptions::{Candidate, Payment, detect_subscriptions};
pub use transaction::{NewSplit, NewTransactionGroup, SplitMeta, ValidatedGroup, ValidatedSplit};
pub use validation::validate_group;

pub use piggy_bank::{
    EventOutcome, PiggyBankId, PiggyBankState, PiggyEventError, check_event, check_imported_event,
    room_to_add,
};
pub use recurrence::{
    Pattern, RecurrenceId, RecurrenceSchedule, Repetition, ScheduleError, WeekendPolicy,
};

/// Name of the application, shared by every crate.
pub const APP_NAME: &str = "Saldo";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_name_is_saldo() {
        assert_eq!(APP_NAME, "Saldo");
    }
}
