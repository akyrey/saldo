//! Transaction groups and their splits: unvalidated input and validated output.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::ids::{AccountId, BillId, BudgetId, CategoryId, HouseholdId, TagId};
use crate::kind::SplitKind;
use crate::money::Money;

/// Free-form extra fields on a split (SEPA data, extra dates, references,
/// `firefly` import data), stored as a JSON object.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SplitMeta(pub serde_json::Map<String, serde_json::Value>);

/// One movement of money from `source` to `destination`, not yet validated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewSplit {
    pub date: NaiveDate,
    pub description: String,
    pub source: AccountId,
    pub destination: AccountId,
    /// Always greater than zero; the direction is source → destination.
    pub amount: Money,
    pub foreign_amount: Option<Money>,
    pub category: Option<CategoryId>,
    pub budget: Option<BudgetId>,
    pub bill: Option<BillId>,
    #[serde(default)]
    pub tags: Vec<TagId>,
    #[serde(default)]
    pub reconciled: bool,
    pub notes: Option<String>,
    #[serde(default)]
    pub meta: SplitMeta,
}

/// One or more splits entered together, not yet validated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewTransactionGroup {
    pub title: Option<String>,
    pub splits: Vec<NewSplit>,
}

/// A split that passed validation, with its derived kind.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ValidatedSplit {
    #[serde(flatten)]
    split: NewSplit,
    kind: SplitKind,
}

impl ValidatedSplit {
    pub fn split(&self) -> &NewSplit {
        &self.split
    }

    pub fn kind(&self) -> SplitKind {
        self.kind
    }
}

/// A transaction group that satisfies every ledger rule.
///
/// "Parse, don't validate": the fields are private and the only way to build
/// one is [`crate::validation::validate_group`], so holding a `ValidatedGroup`
/// proves it was checked. Services in `saldo-app` accept only this type.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ValidatedGroup {
    household_id: HouseholdId,
    title: Option<String>,
    splits: Vec<ValidatedSplit>,
}

impl ValidatedGroup {
    /// `pub(crate)`: visible inside `saldo-core` only, so other crates cannot
    /// skip validation.
    pub(crate) fn new(
        household_id: HouseholdId,
        title: Option<String>,
        splits: Vec<(NewSplit, SplitKind)>,
    ) -> Self {
        let splits = splits
            .into_iter()
            .map(|(split, kind)| ValidatedSplit { split, kind })
            .collect();
        Self {
            household_id,
            title,
            splits,
        }
    }

    pub fn household_id(&self) -> HouseholdId {
        self.household_id
    }

    /// `Option<&str>` borrows the title instead of copying the `String`.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn splits(&self) -> &[ValidatedSplit] {
        &self.splits
    }
}
