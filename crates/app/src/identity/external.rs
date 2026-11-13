//! Login through an external identity provider (Authentik over OpenID
//! Connect). The server checks the provider's signed ID token; this module
//! only decides which Saldo member that person is. Like login and logout, it
//! appends no event (authentication state, not household data).

use saldo_core::{HouseholdId, UserId};
use sqlx::SqliteConnection;

use super::{IdentityService, LoginResult};
use crate::clock::format_timestamp;
use crate::ctx::{HouseholdCtx, Role};
use crate::error::{AppError, parse_stored};

/// Who the provider says logged in, from a verified ID token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalIdentity {
    /// The provider's issuer URL.
    pub issuer: String,
    /// The provider's stable ID of the person (`sub`).
    pub subject: String,
    pub email: Option<String>,
    /// Only a verified email may link a new identity to a member.
    pub email_verified: bool,
}

/// A user with their first household membership.
struct Member {
    user_id: String,
    household_id: String,
    role: String,
}

impl IdentityService {
    /// Opens a session for the member this identity belongs to: the one
    /// linked before, or, on the first login, the member with the same
    /// verified email (case-insensitive), which is then linked. Anyone else
    /// is `Unauthorized`: an owner must add them as a member first.
    pub async fn login_external(
        &self,
        identity: ExternalIdentity,
    ) -> Result<LoginResult, AppError> {
        let mut tx = self.pool.begin().await?;
        let member = match linked_member(&mut tx, &identity).await? {
            Some(member) => member,
            None => {
                let member = member_by_email(&mut tx, &identity)
                    .await?
                    .ok_or(AppError::Unauthorized)?;
                self.link(&mut tx, &identity, &member.user_id).await?;
                member
            }
        };
        tx.commit().await?;
        let ctx = HouseholdCtx {
            household_id: parse_stored::<HouseholdId>(&member.household_id)?,
            user_id: parse_stored::<UserId>(&member.user_id)?,
            role: Role::parse(&member.role)?,
        };
        self.open_session(ctx).await
    }

    async fn link(
        &self,
        conn: &mut SqliteConnection,
        identity: &ExternalIdentity,
        user_id: &str,
    ) -> Result<(), AppError> {
        let now = format_timestamp(self.clock.now());
        sqlx::query!(
            "INSERT INTO user_identities (issuer, subject, user_id, created_at) VALUES (?, ?, ?, ?)",
            identity.issuer,
            identity.subject,
            user_id,
            now,
        )
        .execute(&mut *conn)
        .await?;
        Ok(())
    }
}

/// The member linked to this identity, if they still belong to a household.
async fn linked_member(
    conn: &mut SqliteConnection,
    identity: &ExternalIdentity,
) -> Result<Option<Member>, AppError> {
    let member = sqlx::query_as!(
        Member,
        "SELECT u.id AS user_id, m.household_id, m.role
         FROM user_identities i
         JOIN users u ON u.id = i.user_id
         JOIN household_members m ON m.user_id = u.id
         WHERE i.issuer = ? AND i.subject = ?
         ORDER BY m.created_at LIMIT 1",
        identity.issuer,
        identity.subject,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(member)
}

/// The member with the identity's email, if the provider verified it.
async fn member_by_email(
    conn: &mut SqliteConnection,
    identity: &ExternalIdentity,
) -> Result<Option<Member>, AppError> {
    let Some(email) = identity
        .email
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty())
    else {
        return Ok(None);
    };
    if !identity.email_verified {
        return Ok(None);
    }
    let member = sqlx::query_as!(
        Member,
        "SELECT u.id AS user_id, m.household_id, m.role
         FROM users u JOIN household_members m ON m.user_id = u.id
         WHERE lower(u.email) = lower(?)
         ORDER BY m.created_at LIMIT 1",
        email,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(member)
}
