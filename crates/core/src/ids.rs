//! Typed IDs.
//!
//! Each ID is a *newtype*: a struct wrapping exactly one value. An `AccountId`
//! and a `SplitId` are both UUIDs underneath, but the compiler refuses to accept
//! one where the other is expected. Go's `type AccountID string` is close, except
//! Rust never converts implicitly. PHP would need a small value-object class.
//!
//! The types are written out by hand on purpose: no macros of our own.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::IdError;

// `Result<T, E>` is either `Ok(T)` or `Err(E)`: Go's `(value, err)` pair as one
// value, with the error type written in the signature (unlike PHP exceptions).
fn parse_uuid(input: &str) -> Result<Uuid, IdError> {
    Uuid::parse_str(input).map_err(|_| IdError {
        input: input.to_string(),
    })
}

/// Identifies a household: the group of users that share all data.
// `#[serde(transparent)]` makes the JSON form the inner UUID string,
// not `{"0": "..."}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HouseholdId(pub Uuid);

impl fmt::Display for HouseholdId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for HouseholdId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_uuid(s).map(Self)
    }
}

/// Identifies a user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserId(pub Uuid);

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for UserId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_uuid(s).map(Self)
    }
}

/// Identifies an account (asset, expense, revenue, liability or equity).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccountId(pub Uuid);

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for AccountId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_uuid(s).map(Self)
    }
}

/// Identifies a transaction group (one or more splits entered together).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GroupId(pub Uuid);

impl fmt::Display for GroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for GroupId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_uuid(s).map(Self)
    }
}

/// Identifies one split inside a transaction group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SplitId(pub Uuid);

impl fmt::Display for SplitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for SplitId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_uuid(s).map(Self)
    }
}

/// Identifies a category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CategoryId(pub Uuid);

impl fmt::Display for CategoryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for CategoryId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_uuid(s).map(Self)
    }
}

/// Identifies a tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TagId(pub Uuid);

impl fmt::Display for TagId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for TagId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_uuid(s).map(Self)
    }
}

/// Identifies a budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BudgetId(pub Uuid);

impl fmt::Display for BudgetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for BudgetId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_uuid(s).map(Self)
    }
}

/// Identifies a bill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BillId(pub Uuid);

impl fmt::Display for BillId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for BillId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_uuid(s).map(Self)
    }
}

/// Identifies an attachment (a stored file linked to a split, account, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AttachmentId(pub Uuid);

impl fmt::Display for AttachmentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for AttachmentId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_uuid(s).map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::DeserializeOwned;

    const TEXT: &str = "0190f5c3-7a2b-7c00-8000-000000000001";

    // A generic test helper: `T` is any type implementing the listed traits
    // (traits are Rust's interfaces). Lets one helper check all nine IDs.
    fn assert_round_trips<T>(wrap: fn(Uuid) -> T)
    where
        T: FromStr<Err = IdError>
            + fmt::Display
            + Serialize
            + DeserializeOwned
            + PartialEq
            + fmt::Debug,
    {
        let id = wrap(Uuid::parse_str(TEXT).unwrap());

        assert_eq!(id.to_string(), TEXT);
        assert_eq!(TEXT.parse::<T>().unwrap(), id);

        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{TEXT}\""));
        assert_eq!(serde_json::from_str::<T>(&json).unwrap(), id);

        let err = "not-a-uuid".parse::<T>().unwrap_err();
        assert_eq!(err.input, "not-a-uuid");
        assert!(serde_json::from_str::<T>("\"nope\"").is_err());
    }

    #[test]
    fn every_id_round_trips_through_string_and_json() {
        assert_round_trips(HouseholdId);
        assert_round_trips(UserId);
        assert_round_trips(AccountId);
        assert_round_trips(GroupId);
        assert_round_trips(SplitId);
        assert_round_trips(CategoryId);
        assert_round_trips(TagId);
        assert_round_trips(BudgetId);
        assert_round_trips(BillId);
        assert_round_trips(AttachmentId);
    }

    #[test]
    fn empty_string_is_not_an_id() {
        assert!("".parse::<AccountId>().is_err());
    }

    #[test]
    fn error_message_names_the_input() {
        let err = "x".parse::<AccountId>().unwrap_err();
        assert_eq!(err.to_string(), "invalid id: \"x\"");
    }
}
