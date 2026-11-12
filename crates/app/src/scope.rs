//! The `scope` filter of every list: whose accounts to include.

use saldo_core::UserId;

use crate::ctx::HouseholdCtx;
use crate::error::AppError;

/// `household` (everything), `mine` (accounts the caller owns) or
/// `user:<id>` (accounts a given member owns).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    Household,
    Mine,
    User(UserId),
}

impl Scope {
    /// Parses the query-string form.
    pub fn parse(text: &str) -> Result<Self, AppError> {
        match text {
            "household" => Ok(Scope::Household),
            "mine" => Ok(Scope::Mine),
            _ => text
                .strip_prefix("user:")
                .and_then(|id| id.parse().ok())
                .map(Scope::User)
                .ok_or_else(|| AppError::invalid("scope", "must be household, mine or user:<id>")),
        }
    }

    /// The account owner to filter by, as stored; `None` means no filter.
    pub(crate) fn owner(&self, ctx: &HouseholdCtx) -> Option<String> {
        match self {
            Scope::Household => None,
            Scope::Mine => Some(ctx.user_id.to_string()),
            Scope::User(user_id) => Some(user_id.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::Role;
    use saldo_core::HouseholdId;
    use uuid::Uuid;

    #[test]
    fn parses_the_three_forms() {
        let id = "0190f5c3-7a2b-7c00-8000-000000000001";
        assert_eq!(Scope::parse("household").unwrap(), Scope::Household);
        assert_eq!(Scope::parse("mine").unwrap(), Scope::Mine);
        assert_eq!(
            Scope::parse(&format!("user:{id}")).unwrap(),
            Scope::User(id.parse().unwrap())
        );
        for bad in ["", "all", "user:", "user:nope"] {
            assert!(Scope::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn owner_filter_follows_the_scope() {
        let ctx = HouseholdCtx {
            household_id: HouseholdId(Uuid::from_u128(1)),
            user_id: UserId(Uuid::from_u128(2)),
            role: Role::Member,
        };
        let other = UserId(Uuid::from_u128(3));
        assert_eq!(Scope::Household.owner(&ctx), None);
        assert_eq!(Scope::Mine.owner(&ctx), Some(ctx.user_id.to_string()));
        assert_eq!(Scope::User(other).owner(&ctx), Some(other.to_string()));
    }
}
