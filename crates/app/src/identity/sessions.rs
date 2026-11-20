//! Login sessions and personal access tokens. Both are rows of `sessions`;
//! only the SHA-256 of a token is stored.

use chrono::{DateTime, TimeDelta, Utc};
use saldo_core::{HouseholdId, UserId};

use super::household::user_view;
use super::secrets::{hash_token, new_token, verify_password};
use super::{
    Authenticated, CreatedToken, IdentityService, LoginResult, SessionClient, SessionView,
    TokenView, required,
};
use crate::clock::{format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Role};
use crate::error::{AppError, parse_stored};

/// How long a login session lasts.
pub const SESSION_LIFETIME: TimeDelta = TimeDelta::days(30);

impl IdentityService {
    /// Checks email and password and opens a session. Every failure is the
    /// same `Unauthorized`, so callers cannot tell which part was wrong.
    pub async fn login(
        &self,
        email: &str,
        password: &str,
        client: SessionClient,
    ) -> Result<LoginResult, AppError> {
        let email = email.trim();
        let user = sqlx::query!(
            "SELECT u.id, u.password_hash, m.household_id, m.role
             FROM users u JOIN household_members m ON m.user_id = u.id
             WHERE u.email = ? ORDER BY m.created_at LIMIT 1",
            email,
        )
        .fetch_optional(&self.pool)
        .await?;
        let stored_hash = user.as_ref().map(|u| u.password_hash.clone());
        let valid = verify_password(password.to_string(), stored_hash).await?;
        let Some(user) = user.filter(|_| valid) else {
            return Err(AppError::Unauthorized);
        };

        let ctx = HouseholdCtx {
            household_id: parse_stored::<HouseholdId>(&user.household_id)?,
            user_id: parse_stored::<UserId>(&user.id)?,
            role: Role::parse(&user.role)?,
        };
        self.open_session(ctx, client).await
    }

    /// Opens a login session for `ctx` (after a password or an external
    /// identity provider vouched for the person).
    pub(super) async fn open_session(
        &self,
        ctx: HouseholdCtx,
        client: SessionClient,
    ) -> Result<LoginResult, AppError> {
        let now = self.clock.now();
        let expires_at = format_timestamp(now + SESSION_LIFETIME);
        let token = new_token();
        let client = Some(client.as_str());
        self.insert_session(&ctx, "session", client, &token, Some(&expires_at))
            .await?;
        let mut conn = self.pool.acquire().await?;
        let user = user_view(&mut conn, ctx.user_id).await?;
        Ok(LoginResult {
            token,
            ctx,
            user,
            expires_at,
        })
    }

    /// Resolves a session cookie or bearer token to the caller. Expired
    /// sessions and sessions of removed members are rejected.
    pub async fn authenticate(&self, token: &str) -> Result<Authenticated, AppError> {
        let token_hash = hash_token(token);
        let row = sqlx::query!(
            "SELECT s.id, s.user_id, s.household_id, s.kind, s.expires_at, m.role
             FROM sessions s
             JOIN household_members m ON m.household_id = s.household_id AND m.user_id = s.user_id
             WHERE s.token_hash = ?",
            token_hash,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(AppError::Unauthorized)?;

        let now = self.clock.now();
        if let Some(expires_at) = &row.expires_at {
            let expires_at: DateTime<Utc> = parse_stored(expires_at)?;
            if expires_at <= now {
                sqlx::query!("DELETE FROM sessions WHERE id = ?", row.id)
                    .execute(&self.pool)
                    .await?;
                return Err(AppError::Unauthorized);
            }
        }
        let last_used = format_timestamp(now);
        sqlx::query!(
            "UPDATE sessions SET last_used_at = ? WHERE id = ?",
            last_used,
            row.id
        )
        .execute(&self.pool)
        .await?;
        Ok(Authenticated {
            ctx: HouseholdCtx {
                household_id: parse_stored(&row.household_id)?,
                user_id: parse_stored(&row.user_id)?,
                role: Role::parse(&row.role)?,
            },
            session_id: row.id,
            is_token: row.kind == "token",
        })
    }

    /// Ends one session of the caller.
    pub async fn logout(&self, ctx: &HouseholdCtx, session_id: &str) -> Result<(), AppError> {
        let user_id = ctx.user_id.to_string();
        sqlx::query!(
            "DELETE FROM sessions WHERE id = ? AND user_id = ?",
            session_id,
            user_id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The caller's login sessions that have not expired, newest first.
    /// `current_session_id` is the session making the request.
    pub async fn list_sessions(
        &self,
        ctx: &HouseholdCtx,
        current_session_id: &str,
    ) -> Result<Vec<SessionView>, AppError> {
        let (household_id, user_id) = (ctx.household_id.to_string(), ctx.user_id.to_string());
        let now = format_timestamp(self.clock.now());
        let rows = sqlx::query!(
            r#"SELECT id, name, created_at, last_used_at, expires_at AS "expires_at!" FROM sessions
             WHERE household_id = ? AND user_id = ? AND kind = 'session'
               AND expires_at > ?
             ORDER BY created_at DESC, id DESC"#,
            household_id,
            user_id,
            now,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| SessionView {
                current: row.id == current_session_id,
                client: SessionClient::from_stored(row.name.as_deref()),
                id: row.id,
                created_at: row.created_at,
                last_used_at: row.last_used_at,
                expires_at: row.expires_at,
            })
            .collect())
    }

    /// Ends one of the caller's login sessions (logs that device out).
    /// Tokens are revoked with [`Self::revoke_token`] instead.
    pub async fn end_session(&self, ctx: &HouseholdCtx, id: &str) -> Result<(), AppError> {
        let (household_id, user_id) = (ctx.household_id.to_string(), ctx.user_id.to_string());
        let result = sqlx::query!(
            "DELETE FROM sessions
             WHERE id = ? AND household_id = ? AND user_id = ? AND kind = 'session'",
            id,
            household_id,
            user_id,
        )
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    /// Ends every login session of the caller except `current_session_id`
    /// ("sign out everywhere else"). Tokens stay. Returns how many ended.
    pub async fn end_other_sessions(
        &self,
        ctx: &HouseholdCtx,
        current_session_id: &str,
    ) -> Result<u64, AppError> {
        let (household_id, user_id) = (ctx.household_id.to_string(), ctx.user_id.to_string());
        let result = sqlx::query!(
            "DELETE FROM sessions
             WHERE household_id = ? AND user_id = ? AND kind = 'session' AND id <> ?",
            household_id,
            user_id,
            current_session_id,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Creates a personal access token for the caller. It does not expire.
    pub async fn create_token(
        &self,
        ctx: &HouseholdCtx,
        name: &str,
    ) -> Result<CreatedToken, AppError> {
        let name = required("name", name)?;
        let token = new_token();
        let id = self
            .insert_session(ctx, "token", Some(&name), &token, None)
            .await?;
        let view = self
            .list_tokens(ctx)
            .await?
            .into_iter()
            .find(|t| t.id == id)
            .ok_or(AppError::NotFound)?;
        Ok(CreatedToken { token, view })
    }

    /// The caller's personal access tokens, newest first.
    pub async fn list_tokens(&self, ctx: &HouseholdCtx) -> Result<Vec<TokenView>, AppError> {
        let (household_id, user_id) = (ctx.household_id.to_string(), ctx.user_id.to_string());
        let rows = sqlx::query!(
            "SELECT id, name, created_at, last_used_at, expires_at FROM sessions
             WHERE household_id = ? AND user_id = ? AND kind = 'token'
             ORDER BY created_at DESC, id DESC",
            household_id,
            user_id,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| TokenView {
                id: row.id,
                name: row.name,
                created_at: row.created_at,
                last_used_at: row.last_used_at,
                expires_at: row.expires_at,
            })
            .collect())
    }

    /// Revokes one of the caller's tokens.
    pub async fn revoke_token(&self, ctx: &HouseholdCtx, id: &str) -> Result<(), AppError> {
        let (household_id, user_id) = (ctx.household_id.to_string(), ctx.user_id.to_string());
        let result = sqlx::query!(
            "DELETE FROM sessions
             WHERE id = ? AND household_id = ? AND user_id = ? AND kind = 'token'",
            id,
            household_id,
            user_id,
        )
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    async fn insert_session(
        &self,
        ctx: &HouseholdCtx,
        kind: &str,
        name: Option<&str>,
        token: &str,
        expires_at: Option<&str>,
    ) -> Result<String, AppError> {
        let id = new_id(&*self.clock).to_string();
        let (user_id, household_id) = (ctx.user_id.to_string(), ctx.household_id.to_string());
        let token_hash = hash_token(token);
        let now = format_timestamp(self.clock.now());
        sqlx::query!(
            "INSERT INTO sessions (id, user_id, household_id, kind, name, token_hash, created_at, expires_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            id,
            user_id,
            household_id,
            kind,
            name,
            token_hash,
            now,
            expires_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(id)
    }
}
