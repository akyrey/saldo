//! Statement profiles: a household's saved way of reading one bank's export
//! (Phase 13, Track B).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqliteConnection;

use super::parse::Mapping;
use super::profile_repo::{self, ProfileRow};
use super::{StatementService, account_type};
use crate::clock::{format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::AppError;
use crate::events::{NewEvent, append_event};

const ENTITY: &str = "statement_profile";
const MAX_NAME_CHARS: usize = 100;
const MAX_TRANSFER_RULES: usize = 50;
const MAX_PATTERN_CHARS: usize = 200;

/// A line whose description contains `pattern` (ignoring case) is a transfer
/// with one of `account_ids`, the household's own accounts (brief
/// decision 3: a bank line mentioning Satispay is a top-up).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TransferRule {
    pub pattern: String,
    pub account_ids: Vec<String>,
}

/// A profile as the client sends it.
#[derive(Debug, Clone)]
pub struct ProfileInput {
    pub name: String,
    /// The asset or liability account files are imported into by default.
    pub account_id: Option<String>,
    /// `csv` or `xlsx`.
    pub format: String,
    pub mapping: Mapping,
    /// Part column header → asset account, or `None` to keep that part as a
    /// note (brief decision 13).
    pub part_accounts: BTreeMap<String, Option<String>>,
    pub transfer_rules: Vec<TransferRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct ProfileView {
    pub id: String,
    pub name: String,
    pub account_id: Option<String>,
    pub format: String,
    /// The column mapping (see `saldo_app::statements::parse::Mapping`).
    #[schema(value_type = Object)]
    pub mapping: Value,
    /// Part column header → account ID or null.
    #[schema(value_type = Object)]
    pub part_accounts: Value,
    pub transfer_rules: Vec<TransferRule>,
    pub created_at: String,
    pub updated_at: String,
}

/// A built-in mapping offered when creating a profile.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct Preset {
    pub name: String,
    pub format: String,
    #[schema(value_type = Object)]
    pub mapping: Value,
}

/// The mappings of the owner's banks, offered when creating a profile.
pub fn presets() -> Vec<Preset> {
    [
        ("Widiba", Mapping::widiba()),
        ("Mediobanca", Mapping::mediobanca()),
        ("Satispay", Mapping::satispay()),
    ]
    .into_iter()
    .map(|(name, mapping)| Preset {
        name: name.to_string(),
        format: "xlsx".to_string(),
        mapping: serde_json::to_value(mapping).unwrap_or(Value::Null),
    })
    .collect()
}

impl StatementService {
    /// Profiles by name.
    pub async fn list_profiles(&self, ctx: &HouseholdCtx) -> Result<Vec<ProfileView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        profile_repo::list(&mut conn, &ctx.household_id.to_string())
            .await?
            .into_iter()
            .map(view_of)
            .collect()
    }

    pub async fn get_profile(&self, ctx: &HouseholdCtx, id: &str) -> Result<ProfileView, AppError> {
        let mut conn = self.pool.acquire().await?;
        view_of(profile_repo::find(&mut conn, &ctx.household_id.to_string(), id).await?)
    }

    pub async fn create_profile(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: ProfileInput,
    ) -> Result<ProfileView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let household_id = ctx.household_id.to_string();
        let now = format_timestamp(self.clock.now());
        let row = checked_row(
            &mut tx,
            &household_id,
            new_id(&*self.clock).to_string(),
            &now,
            &now,
            input,
        )
        .await?;
        profile_repo::insert(&mut tx, &household_id, &row).await?;
        let view = view_of(row)?;
        self.emit_profile(&mut tx, ctx, origin, &view, "created")
            .await?;
        tx.commit().await?;
        Ok(view)
    }

    /// Replaces every field of a profile.
    pub async fn update_profile(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
        input: ProfileInput,
    ) -> Result<ProfileView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let household_id = ctx.household_id.to_string();
        let old = profile_repo::find(&mut tx, &household_id, id).await?;
        let now = format_timestamp(self.clock.now());
        let row = checked_row(&mut tx, &household_id, old.id, &old.created_at, &now, input).await?;
        profile_repo::update(&mut tx, &household_id, &row).await?;
        let view = view_of(row)?;
        self.emit_profile(&mut tx, ctx, origin, &view, "updated")
            .await?;
        tx.commit().await?;
        Ok(view)
    }

    /// Deletes a profile; imports made with it keep their lines.
    pub async fn delete_profile(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
    ) -> Result<(), AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let household_id = ctx.household_id.to_string();
        let view = view_of(profile_repo::find(&mut tx, &household_id, id).await?)?;
        profile_repo::delete(&mut tx, &household_id, id).await?;
        self.emit_profile(&mut tx, ctx, origin, &view, "deleted")
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn emit_profile(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        view: &ProfileView,
        verb: &str,
    ) -> Result<(), AppError> {
        let event = NewEvent::new(&*self.clock, ENTITY, verb, view.id.clone(), view)?;
        append_event(conn, ctx, origin, event).await?;
        Ok(())
    }
}

/// The row to store, once every field is checked.
async fn checked_row(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: String,
    created_at: &str,
    updated_at: &str,
    input: ProfileInput,
) -> Result<ProfileRow, AppError> {
    let name = input.name.trim().to_string();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(AppError::invalid(
            "name",
            format!("must be 1 to {MAX_NAME_CHARS} characters"),
        ));
    }
    if !matches!(input.format.as_str(), "csv" | "xlsx") {
        return Err(AppError::invalid("format", "must be csv or xlsx"));
    }
    input
        .mapping
        .validate()
        .map_err(|error| AppError::invalid("mapping", error.to_string()))?;
    if let Some(account) = &input.account_id {
        statement_account(conn, household_id, account, "account_id").await?;
    }
    check_parts(conn, household_id, &input.mapping, &input.part_accounts).await?;
    check_transfer_rules(conn, household_id, &input.transfer_rules).await?;
    let json = |value: Result<String, serde_json::Error>| {
        value.map_err(|e| AppError::Internal(e.to_string()))
    };
    Ok(ProfileRow {
        id,
        name,
        account_id: input.account_id,
        format: input.format,
        mapping: json(serde_json::to_string(&input.mapping))?,
        part_accounts: json(serde_json::to_string(&input.part_accounts))?,
        transfer_rules: json(serde_json::to_string(&input.transfer_rules))?,
        created_at: created_at.to_string(),
        updated_at: updated_at.to_string(),
    })
}

/// The account a statement can belong to: the household's asset or
/// liability account.
async fn statement_account(
    conn: &mut SqliteConnection,
    household_id: &str,
    account: &str,
    field: &str,
) -> Result<(), AppError> {
    match account_type(conn, household_id, account).await?.as_deref() {
        Some("asset" | "liability") => Ok(()),
        Some(_) => Err(AppError::invalid(
            field,
            "must be an asset or liability account",
        )),
        None => Err(AppError::invalid(
            field,
            format!("{account} is not an account of this household"),
        )),
    }
}

/// Every key names one of the mapping's part columns; every account is the
/// household's asset account.
async fn check_parts(
    conn: &mut SqliteConnection,
    household_id: &str,
    mapping: &Mapping,
    parts: &BTreeMap<String, Option<String>>,
) -> Result<(), AppError> {
    let field = "part_accounts";
    for (column, account) in parts {
        let known = mapping
            .part_columns
            .iter()
            .flatten()
            .any(|name| name.trim().eq_ignore_ascii_case(column.trim()));
        if !known {
            return Err(AppError::invalid(
                field,
                format!("{column:?} is not a part column of the mapping"),
            ));
        }
        let Some(account) = account else { continue };
        if account_type(conn, household_id, account).await?.as_deref() != Some("asset") {
            return Err(AppError::invalid(
                field,
                format!("{column:?} must go to an asset account of this household"),
            ));
        }
    }
    Ok(())
}

async fn check_transfer_rules(
    conn: &mut SqliteConnection,
    household_id: &str,
    rules: &[TransferRule],
) -> Result<(), AppError> {
    let field = "transfer_rules";
    if rules.len() > MAX_TRANSFER_RULES {
        return Err(AppError::invalid(
            field,
            format!("at most {MAX_TRANSFER_RULES} rules"),
        ));
    }
    for rule in rules {
        let pattern = rule.pattern.trim();
        if pattern.is_empty() || pattern.chars().count() > MAX_PATTERN_CHARS {
            return Err(AppError::invalid(
                field,
                format!("a pattern must be 1 to {MAX_PATTERN_CHARS} characters"),
            ));
        }
        if rule.account_ids.is_empty() || rule.account_ids.len() > MAX_TRANSFER_RULES {
            return Err(AppError::invalid(
                field,
                "a rule names one or more accounts",
            ));
        }
        for account in &rule.account_ids {
            statement_account(conn, household_id, account, field).await?;
        }
    }
    Ok(())
}

fn view_of(row: ProfileRow) -> Result<ProfileView, AppError> {
    let parse = |text: &str| -> Result<Value, AppError> {
        serde_json::from_str(text).map_err(|e| AppError::Internal(format!("stored profile: {e}")))
    };
    let transfer_rules = serde_json::from_str(&row.transfer_rules)
        .map_err(|e| AppError::Internal(format!("stored profile: {e}")))?;
    Ok(ProfileView {
        mapping: parse(&row.mapping)?,
        part_accounts: parse(&row.part_accounts)?,
        transfer_rules,
        id: row.id,
        name: row.name,
        account_id: row.account_id,
        format: row.format,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
