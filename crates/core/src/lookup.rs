//! What a validation needs to know about the database, loaded by the caller.

use std::collections::{HashMap, HashSet};

use crate::account::Account;
use crate::ids::{AccountId, BillId, BudgetId, CategoryId, HouseholdId, TagId};

/// The household a group is being validated for, the accounts its splits
/// refer to, and the IDs of the categories, budgets, bills and tags that exist
/// in that household.
///
/// `saldo-core` cannot query a database, so the service loads these first
/// and hands them over. Anything missing from the lookup counts as not
/// existing, so a forgotten load fails validation instead of passing it.
///
/// The lookup owns copies of the accounts (no borrowed references), which
/// keeps the type free of lifetime annotations.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountLookup {
    household_id: HouseholdId,
    accounts: HashMap<AccountId, Account>,
    categories: HashSet<CategoryId>,
    budgets: HashSet<BudgetId>,
    bills: HashSet<BillId>,
    tags: HashSet<TagId>,
}

impl AccountLookup {
    pub fn new(household_id: HouseholdId) -> Self {
        Self {
            household_id,
            accounts: HashMap::new(),
            categories: HashSet::new(),
            budgets: HashSet::new(),
            bills: HashSet::new(),
            tags: HashSet::new(),
        }
    }

    // The `with_*` methods take `self` by value and return it changed: a
    // builder, like Laravel's fluent query builder. Taking `self` by value
    // *moves* the lookup in, so no one else can be holding the old copy.

    pub fn with_account(mut self, account: Account) -> Self {
        self.accounts.insert(account.id, account);
        self
    }

    pub fn with_category(mut self, id: CategoryId) -> Self {
        self.categories.insert(id);
        self
    }

    pub fn with_budget(mut self, id: BudgetId) -> Self {
        self.budgets.insert(id);
        self
    }

    pub fn with_bill(mut self, id: BillId) -> Self {
        self.bills.insert(id);
        self
    }

    pub fn with_tag(mut self, id: TagId) -> Self {
        self.tags.insert(id);
        self
    }

    pub fn household_id(&self) -> HouseholdId {
        self.household_id
    }

    pub fn account(&self, id: AccountId) -> Option<&Account> {
        self.accounts.get(&id)
    }

    pub fn has_category(&self, id: CategoryId) -> bool {
        self.categories.contains(&id)
    }

    pub fn has_budget(&self, id: BudgetId) -> bool {
        self.budgets.contains(&id)
    }

    pub fn has_bill(&self, id: BillId) -> bool {
        self.bills.contains(&id)
    }

    pub fn has_tag(&self, id: TagId) -> bool {
        self.tags.contains(&id)
    }
}
