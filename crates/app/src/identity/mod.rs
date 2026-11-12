//! Identity: setup, household, members, login sessions and API tokens.

mod household;
mod secrets;
mod sessions;

pub use sessions::SESSION_LIFETIME;

use serde::Serialize;
use sqlx::SqlitePool;

use crate::clock::SharedClock;
use crate::ctx::{HouseholdCtx, Role};
use crate::error::AppError;

pub const MIN_PASSWORD_LENGTH: usize = 8;

/// Input for the very first user and household.
#[derive(Debug, Clone)]
pub struct SetupInput {
    pub email: String,
    pub password: String,
    pub display_name: String,
    pub household_name: String,
    pub primary_currency: String,
}

/// Input for adding a household member with an initial password.
#[derive(Debug, Clone)]
pub struct NewMember {
    pub email: String,
    pub display_name: String,
    pub password: String,
}

/// Changes to household settings; `None` leaves a field as it is.
#[derive(Debug, Clone, Default)]
pub struct HouseholdUpdate {
    pub name: Option<String>,
    pub timezone: Option<String>,
    pub primary_currency: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct UserView {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct HouseholdView {
    pub id: String,
    pub name: String,
    pub primary_currency: String,
    pub timezone: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct MemberView {
    pub user_id: String,
    pub email: String,
    pub display_name: String,
    pub role: Role,
    pub joined_at: String,
}

/// The caller, their household and their role (`GET /auth/me`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct MeView {
    pub user: UserView,
    pub household: HouseholdView,
    pub role: Role,
}

/// A personal access token without its secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct TokenView {
    pub id: String,
    pub name: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub expires_at: Option<String>,
}

/// Result of `setup`: the new owner's context and views.
#[derive(Debug, Clone)]
pub struct SetupResult {
    pub ctx: HouseholdCtx,
    pub user: UserView,
    pub household: HouseholdView,
}

/// Result of a login. `token` is shown once and never stored in clear.
#[derive(Debug, Clone)]
pub struct LoginResult {
    pub token: String,
    pub ctx: HouseholdCtx,
    pub user: UserView,
    pub expires_at: String,
}

/// A request authenticated by a session or token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authenticated {
    pub ctx: HouseholdCtx,
    pub session_id: String,
    /// True for a personal access token, false for a login session.
    pub is_token: bool,
}

/// A newly created personal access token; `token` is shown once.
#[derive(Debug, Clone)]
pub struct CreatedToken {
    pub token: String,
    pub view: TokenView,
}

/// Users, households, members, sessions and tokens.
#[derive(Clone)]
pub struct IdentityService {
    pool: SqlitePool,
    clock: SharedClock,
}

impl IdentityService {
    pub fn new(pool: SqlitePool, clock: SharedClock) -> Self {
        Self { pool, clock }
    }
}

/// Trims `value` and rejects it if empty.
pub(crate) fn required(field: &str, value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::invalid(field, "must not be empty"));
    }
    Ok(trimmed.to_string())
}

fn check_email(email: &str) -> Result<String, AppError> {
    let email = required("email", email)?;
    let valid = email
        .split_once('@')
        .is_some_and(|(user, domain)| !user.is_empty() && domain.contains('.'));
    if !valid {
        return Err(AppError::invalid("email", "is not an email address"));
    }
    Ok(email)
}

fn check_password(password: &str) -> Result<(), AppError> {
    if password.chars().count() < MIN_PASSWORD_LENGTH {
        return Err(AppError::invalid(
            "password",
            format!("must have at least {MIN_PASSWORD_LENGTH} characters"),
        ));
    }
    Ok(())
}

/// The household's primary currency code.
pub(crate) async fn primary_currency(
    conn: &mut sqlx::SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<String, AppError> {
    let id = ctx.household_id.to_string();
    sqlx::query_scalar!("SELECT primary_currency FROM households WHERE id = ?", id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_trims_and_rejects_blank() {
        assert_eq!(required("name", "  Home ").unwrap(), "Home");
        assert!(required("name", "   ").is_err());
    }

    #[test]
    fn emails_need_a_user_and_a_dotted_domain() {
        assert_eq!(check_email(" a@b.co ").unwrap(), "a@b.co");
        for bad in ["", "ab.co", "@b.co", "a@b"] {
            assert!(check_email(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn passwords_need_eight_characters() {
        assert!(check_password("1234567").is_err());
        assert!(check_password("12345678").is_ok());
    }
}
