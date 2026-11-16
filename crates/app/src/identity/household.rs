//! Setup, household settings and members.

use saldo_core::{HouseholdId, UserId};
use sqlx::SqliteConnection;

use super::secrets::hash_password;
use super::{
    HouseholdUpdate, HouseholdView, IdentityService, MeView, MemberView, NewMember, SetupInput,
    SetupResult, UserView, check_email, check_password, required,
};
use crate::accounts::repo::{self as account_repo, AccountRow};
use crate::clock::{format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin, Role};
use crate::currencies;
use crate::error::{AppError, parse_stored};
use crate::events::{NewEvent, append_event};

impl IdentityService {
    /// Creates the first user, their household and its two equity accounts.
    /// Only works while no user exists.
    pub async fn setup(&self, origin: Origin, input: SetupInput) -> Result<SetupResult, AppError> {
        let email = check_email(&input.email)?;
        let display_name = required("display_name", &input.display_name)?;
        let household_name = required("household_name", &input.household_name)?;
        check_password(&input.password)?;
        let password_hash = hash_password(input.password).await?;

        let now = format_timestamp(self.clock.now());
        let ctx = HouseholdCtx {
            household_id: HouseholdId(new_id(&*self.clock)),
            user_id: UserId(new_id(&*self.clock)),
            role: Role::Owner,
        };
        let mut tx = crate::db::begin(&self.pool).await?;
        let currency =
            currencies::find(&mut tx, &input.primary_currency, "primary_currency").await?;
        let currency = currency.code().to_string();
        let household_id = ctx.household_id.to_string();
        let user_id = ctx.user_id.to_string();

        sqlx::query!(
            "INSERT INTO households (id, name, primary_currency, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)",
            household_id,
            household_name,
            currency,
            now,
            now,
        )
        .execute(&mut *tx)
        .await?;
        insert_user(
            &mut tx,
            &user_id,
            &email,
            &display_name,
            &password_hash,
            &now,
        )
        .await?;
        // Inserting first and counting afterwards also catches two setups
        // racing each other: the second one sees two users and rolls back.
        let users = sqlx::query_scalar!("SELECT count(*) FROM users")
            .fetch_one(&mut *tx)
            .await?;
        if users > 1 {
            return Err(AppError::Conflict(
                "setup has already been done".to_string(),
            ));
        }
        insert_member(&mut tx, &household_id, &user_id, Role::Owner, &now).await?;

        let household = household_view(&mut tx, &ctx).await?;
        let member = member_view(&mut tx, &ctx, ctx.user_id).await?;
        let clock = &*self.clock;
        append_event(
            &mut tx,
            &ctx,
            origin,
            NewEvent::new(
                clock,
                "household",
                "created",
                household_id.clone(),
                &household,
            )?,
        )
        .await?;
        append_event(
            &mut tx,
            &ctx,
            origin,
            NewEvent::new(
                clock,
                "household_member",
                "created",
                user_id.clone(),
                &member,
            )?,
        )
        .await?;
        for (name, role) in [
            ("Opening balances", "opening_balance"),
            ("Reconciliation", "reconciliation"),
        ] {
            let row = equity_account(&ctx, clock, name, role, &currency, &now);
            account_repo::insert(&mut tx, &row).await?;
            let view = crate::accounts::view_of(&mut tx, row).await?;
            append_event(
                &mut tx,
                &ctx,
                origin,
                NewEvent::new(clock, "account", "created", view.id.clone(), &view)?,
            )
            .await?;
        }
        let user = user_view(&mut tx, ctx.user_id).await?;
        tx.commit().await?;
        Ok(SetupResult {
            ctx,
            user,
            household,
        })
    }

    /// The context for command-line tools (importer, export): the household
    /// with this ID or name, acting as its first owner.
    pub async fn household_context(&self, id_or_name: &str) -> Result<HouseholdCtx, AppError> {
        let rows = sqlx::query!(
            "SELECT h.id, m.user_id FROM households h
             JOIN household_members m ON m.household_id = h.id AND m.role = 'owner'
             WHERE h.id = ?1 OR h.name = ?1
             ORDER BY m.created_at",
            id_or_name,
        )
        .fetch_all(&self.pool)
        .await?;
        let Some(first) = rows.first() else {
            return Err(AppError::NotFound);
        };
        if rows.iter().any(|r| r.id != first.id) {
            return Err(AppError::Conflict(format!(
                "several households are called {id_or_name:?}; use the ID"
            )));
        }
        Ok(HouseholdCtx {
            household_id: parse_stored(&first.id)?,
            user_id: parse_stored(&first.user_id)?,
            role: Role::Owner,
        })
    }

    /// The caller, their household and their role.
    pub async fn me(&self, ctx: &HouseholdCtx) -> Result<MeView, AppError> {
        let mut conn = self.pool.acquire().await?;
        Ok(MeView {
            user: user_view(&mut conn, ctx.user_id).await?,
            household: household_view(&mut conn, ctx).await?,
            role: ctx.role,
        })
    }

    pub async fn get_household(&self, ctx: &HouseholdCtx) -> Result<HouseholdView, AppError> {
        let mut conn = self.pool.acquire().await?;
        household_view(&mut conn, ctx).await
    }

    /// Changes household settings. Owner only.
    pub async fn update_household(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        update: HouseholdUpdate,
    ) -> Result<HouseholdView, AppError> {
        ctx.require_owner()?;
        let mut tx = crate::db::begin(&self.pool).await?;
        let current = household_view(&mut tx, ctx).await?;
        let name = match update.name {
            Some(name) => required("name", &name)?,
            None => current.name,
        };
        let timezone = match update.timezone {
            Some(timezone) => {
                let name = required("timezone", &timezone)?;
                super::time::parse_timezone("timezone", &name)?;
                name
            }
            None => current.timezone,
        };
        let primary_currency = match update.primary_currency {
            Some(code) => currencies::find(&mut tx, &code, "primary_currency")
                .await?
                .code()
                .to_string(),
            None => current.primary_currency,
        };
        let now = format_timestamp(self.clock.now());
        let household_id = ctx.household_id.to_string();
        sqlx::query!(
            "UPDATE households SET name = ?, timezone = ?, primary_currency = ?, updated_at = ?
             WHERE id = ?",
            name,
            timezone,
            primary_currency,
            now,
            household_id,
        )
        .execute(&mut *tx)
        .await?;
        let view = household_view(&mut tx, ctx).await?;
        let event = NewEvent::new(&*self.clock, "household", "updated", household_id, &view)?;
        append_event(&mut tx, ctx, origin, event).await?;
        tx.commit().await?;
        Ok(view)
    }

    pub async fn list_members(&self, ctx: &HouseholdCtx) -> Result<Vec<MemberView>, AppError> {
        let household_id = ctx.household_id.to_string();
        let rows = sqlx::query!(
            "SELECT u.id, u.email, u.display_name, m.role, m.created_at
             FROM household_members m JOIN users u ON u.id = m.user_id
             WHERE m.household_id = ? ORDER BY m.created_at, u.email",
            household_id,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(MemberView {
                    user_id: row.id,
                    email: row.email,
                    display_name: row.display_name,
                    role: Role::parse(&row.role)?,
                    joined_at: row.created_at,
                })
            })
            .collect()
    }

    /// Adds a member with an initial password. Owner only.
    pub async fn add_member(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewMember,
    ) -> Result<MemberView, AppError> {
        ctx.require_owner()?;
        let email = check_email(&input.email)?;
        let display_name = required("display_name", &input.display_name)?;
        check_password(&input.password)?;
        let password_hash = hash_password(input.password).await?;

        let user_id = UserId(new_id(&*self.clock));
        let now = format_timestamp(self.clock.now());
        let mut tx = crate::db::begin(&self.pool).await?;
        let id = user_id.to_string();
        insert_user(&mut tx, &id, &email, &display_name, &password_hash, &now).await?;
        insert_member(
            &mut tx,
            &ctx.household_id.to_string(),
            &id,
            Role::Member,
            &now,
        )
        .await?;
        let view = member_view(&mut tx, ctx, user_id).await?;
        let event = NewEvent::new(&*self.clock, "household_member", "created", id, &view)?;
        append_event(&mut tx, ctx, origin, event).await?;
        tx.commit().await?;
        Ok(view)
    }

    /// Removes a member and ends all their sessions and tokens. Owner only;
    /// an owner cannot remove themselves.
    pub async fn remove_member(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        user_id: UserId,
    ) -> Result<(), AppError> {
        ctx.require_owner()?;
        if user_id == ctx.user_id {
            return Err(AppError::Conflict(
                "owners cannot remove themselves".to_string(),
            ));
        }
        let mut tx = crate::db::begin(&self.pool).await?;
        let view = member_view(&mut tx, ctx, user_id).await?;
        let (household_id, id) = (ctx.household_id.to_string(), user_id.to_string());
        sqlx::query!(
            "DELETE FROM household_members WHERE household_id = ? AND user_id = ?",
            household_id,
            id,
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            "DELETE FROM sessions WHERE household_id = ? AND user_id = ?",
            household_id,
            id
        )
        .execute(&mut *tx)
        .await?;
        let event = NewEvent::new(&*self.clock, "household_member", "deleted", id, &view)?;
        append_event(&mut tx, ctx, origin, event).await?;
        tx.commit().await?;
        Ok(())
    }
}

async fn insert_user(
    conn: &mut SqliteConnection,
    id: &str,
    email: &str,
    display_name: &str,
    password_hash: &str,
    now: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO users (id, email, display_name, password_hash, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)",
        id,
        email,
        display_name,
        password_hash,
        now,
        now,
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| AppError::conflict_on_unique(e, "this email is already in use"))?;
    Ok(())
}

async fn insert_member(
    conn: &mut SqliteConnection,
    household_id: &str,
    user_id: &str,
    role: Role,
    now: &str,
) -> Result<(), AppError> {
    let role = role.as_str();
    sqlx::query!(
        "INSERT INTO household_members (household_id, user_id, role, created_at) VALUES (?, ?, ?, ?)",
        household_id, user_id, role, now,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

fn equity_account(
    ctx: &HouseholdCtx,
    clock: &dyn crate::clock::Clock,
    name: &str,
    role: &str,
    currency: &str,
    now: &str,
) -> AccountRow {
    AccountRow {
        id: new_id(clock).to_string(),
        household_id: ctx.household_id.to_string(),
        owner_user_id: None,
        name: name.to_string(),
        account_type: "equity".to_string(),
        equity_role: Some(role.to_string()),
        asset_role: None,
        liability_type: None,
        liability_direction: None,
        interest_rate: None,
        interest_period: None,
        currency_code: currency.to_string(),
        iban: None,
        bic: None,
        account_number: None,
        virtual_balance_minor: 0,
        include_net_worth: false,
        active: true,
        sort_order: 0,
        notes: None,
        created_at: now.to_string(),
        updated_at: now.to_string(),
    }
}

pub(crate) async fn user_view(
    conn: &mut SqliteConnection,
    user_id: UserId,
) -> Result<UserView, AppError> {
    let id = user_id.to_string();
    let row = sqlx::query!(
        "SELECT id, email, display_name, created_at FROM users WHERE id = ?",
        id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(UserView {
        id: row.id,
        email: row.email,
        display_name: row.display_name,
        created_at: row.created_at,
    })
}

pub(crate) async fn household_view(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<HouseholdView, AppError> {
    let id = ctx.household_id.to_string();
    let row = sqlx::query!(
        "SELECT id, name, primary_currency, timezone, created_at, updated_at
         FROM households WHERE id = ?",
        id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(HouseholdView {
        id: row.id,
        name: row.name,
        primary_currency: row.primary_currency,
        timezone: row.timezone,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

async fn member_view(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    user_id: UserId,
) -> Result<MemberView, AppError> {
    let (household_id, id) = (ctx.household_id.to_string(), user_id.to_string());
    let row = sqlx::query!(
        "SELECT u.id, u.email, u.display_name, m.role, m.created_at
         FROM household_members m JOIN users u ON u.id = m.user_id
         WHERE m.household_id = ? AND m.user_id = ?",
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(MemberView {
        user_id: parse_stored::<UserId>(&row.id)?.to_string(),
        email: row.email,
        display_name: row.display_name,
        role: Role::parse(&row.role)?,
        joined_at: row.created_at,
    })
}

/// Every member of the household, inside the caller's transaction.
pub(crate) async fn members_in(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<Vec<MemberView>, AppError> {
    let household_id = ctx.household_id.to_string();
    let rows = sqlx::query!(
        "SELECT u.id, u.email, u.display_name, m.role, m.created_at
         FROM household_members m JOIN users u ON u.id = m.user_id
         WHERE m.household_id = ? ORDER BY m.created_at, u.email",
        household_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(MemberView {
                user_id: row.id,
                email: row.email,
                display_name: row.display_name,
                role: Role::parse(&row.role)?,
                joined_at: row.created_at,
            })
        })
        .collect()
}
