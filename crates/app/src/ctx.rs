//! Who is acting, in which household, and through which channel.

use saldo_core::{HouseholdId, UserId};
use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// A member's role in their household.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Member,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Member => "member",
        }
    }

    pub fn parse(text: &str) -> Result<Self, AppError> {
        match text {
            "owner" => Ok(Role::Owner),
            "member" => Ok(Role::Member),
            other => Err(AppError::Internal(format!("unknown role {other:?}"))),
        }
    }
}

/// The caller of every service and repository function: which user, in which
/// household. Every query filters by `household_id`, so there is no way to
/// reach another household's data. (Laravel: a global scope that cannot be
/// removed. Go: a required typed argument instead of a `context.Context` key.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HouseholdCtx {
    pub household_id: HouseholdId,
    pub user_id: UserId,
    pub role: Role,
}

impl HouseholdCtx {
    /// Owner-only actions: managing members and household settings.
    pub fn require_owner(&self) -> Result<(), AppError> {
        match self.role {
            Role::Owner => Ok(()),
            Role::Member => Err(AppError::Forbidden),
        }
    }
}

/// Which channel caused a change; stored on every event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Api,
    Import,
    Scheduler,
    Rule,
    System,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::Api => "api",
            Origin::Import => "import",
            Origin::Scheduler => "scheduler",
            Origin::Rule => "rule",
            Origin::System => "system",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn ctx(role: Role) -> HouseholdCtx {
        HouseholdCtx {
            household_id: HouseholdId(Uuid::from_u128(1)),
            user_id: UserId(Uuid::from_u128(2)),
            role,
        }
    }

    #[test]
    fn only_owners_pass_require_owner() {
        assert!(ctx(Role::Owner).require_owner().is_ok());
        assert!(matches!(
            ctx(Role::Member).require_owner(),
            Err(AppError::Forbidden)
        ));
    }

    #[test]
    fn roles_round_trip_through_text() {
        for role in [Role::Owner, Role::Member] {
            assert_eq!(Role::parse(role.as_str()).unwrap(), role);
        }
        assert!(Role::parse("admin").is_err());
    }

    #[test]
    fn origins_match_the_sql_values() {
        let names: Vec<_> = [
            Origin::Api,
            Origin::Import,
            Origin::Scheduler,
            Origin::Rule,
            Origin::System,
        ]
        .iter()
        .map(|o| o.as_str())
        .collect();
        assert_eq!(names, ["api", "import", "scheduler", "rule", "system"]);
    }
}
