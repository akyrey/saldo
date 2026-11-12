//! Shared fixtures for unit tests: one household with a handful of accounts.

use chrono::NaiveDate;
use uuid::Uuid;

use crate::account::{Account, AccountType, EquityRole};
use crate::currency::CurrencyCode;
use crate::ids::{AccountId, BillId, BudgetId, CategoryId, HouseholdId, TagId};
use crate::lookup::AccountLookup;
use crate::money::Money;
use crate::transaction::{NewSplit, SplitMeta};

pub const CHECKING: u128 = 10; // EUR asset
pub const SAVINGS: u128 = 11; // EUR asset
pub const USD_WALLET: u128 = 12; // USD asset
pub const CARD: u128 = 13; // EUR liability
pub const GROCERIES: u128 = 20; // EUR expense
pub const SALARY: u128 = 30; // EUR revenue
pub const OPENING: u128 = 40; // EUR equity, opening balance
pub const RECONCILIATION: u128 = 41; // EUR equity, reconciliation
pub const ARCHIVED: u128 = 50; // inactive EUR asset
pub const NEIGHBOUR: u128 = 60; // EUR asset of another household
pub const MISSING: u128 = 99; // not in the lookup

pub const CATEGORY: u128 = 100;
pub const BUDGET: u128 = 101;
pub const BILL: u128 = 102;
pub const TAG: u128 = 103;

pub fn household() -> HouseholdId {
    HouseholdId(Uuid::from_u128(1))
}

pub fn id(n: u128) -> AccountId {
    AccountId(Uuid::from_u128(n))
}

pub fn code(text: &str) -> CurrencyCode {
    CurrencyCode::new(text).unwrap()
}

pub fn eur(minor: i64) -> Money {
    Money::new(minor, code("EUR"))
}

pub fn usd(minor: i64) -> Money {
    Money::new(minor, code("USD"))
}

pub fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, day).unwrap()
}

fn account(n: u128, account_type: AccountType, currency: &str) -> Account {
    Account {
        id: id(n),
        household_id: household(),
        owner: None,
        name: format!("account {n}"),
        account_type,
        currency: code(currency),
        active: true,
    }
}

pub fn accounts() -> Vec<Account> {
    vec![
        account(CHECKING, AccountType::Asset, "EUR"),
        account(SAVINGS, AccountType::Asset, "EUR"),
        account(USD_WALLET, AccountType::Asset, "USD"),
        account(CARD, AccountType::Liability, "EUR"),
        account(GROCERIES, AccountType::Expense, "EUR"),
        account(SALARY, AccountType::Revenue, "EUR"),
        account(
            OPENING,
            AccountType::Equity(EquityRole::OpeningBalance),
            "EUR",
        ),
        account(
            RECONCILIATION,
            AccountType::Equity(EquityRole::Reconciliation),
            "EUR",
        ),
        Account {
            active: false,
            ..account(ARCHIVED, AccountType::Asset, "EUR")
        },
        Account {
            household_id: HouseholdId(Uuid::from_u128(2)),
            ..account(NEIGHBOUR, AccountType::Asset, "EUR")
        },
    ]
}

pub fn lookup() -> AccountLookup {
    accounts()
        .into_iter()
        .fold(AccountLookup::new(household()), AccountLookup::with_account)
        .with_category(CategoryId(Uuid::from_u128(CATEGORY)))
        .with_budget(BudgetId(Uuid::from_u128(BUDGET)))
        .with_bill(BillId(Uuid::from_u128(BILL)))
        .with_tag(TagId(Uuid::from_u128(TAG)))
}

pub fn split(source: u128, destination: u128, amount: Money) -> NewSplit {
    NewSplit {
        date: date(13),
        description: "test".to_string(),
        source: id(source),
        destination: id(destination),
        amount,
        foreign_amount: None,
        category: None,
        budget: None,
        bill: None,
        tags: Vec::new(),
        reconciled: false,
        notes: None,
        meta: SplitMeta::default(),
    }
}
