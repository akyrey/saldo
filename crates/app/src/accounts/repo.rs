//! SQL for the `accounts` table. Every function filters by household.

use sqlx::SqliteConnection;

use crate::error::AppError;

/// One `accounts` row exactly as stored.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AccountRow {
    pub id: String,
    pub household_id: String,
    pub owner_user_id: Option<String>,
    pub name: String,
    pub account_type: String,
    pub equity_role: Option<String>,
    pub asset_role: Option<String>,
    pub liability_type: Option<String>,
    pub liability_direction: Option<String>,
    pub interest_rate: Option<String>,
    pub interest_period: Option<String>,
    pub currency_code: String,
    pub iban: Option<String>,
    pub bic: Option<String>,
    pub account_number: Option<String>,
    pub virtual_balance_minor: i64,
    pub include_net_worth: bool,
    pub active: bool,
    pub sort_order: i64,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub(crate) async fn insert(conn: &mut SqliteConnection, row: &AccountRow) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO accounts (id, household_id, owner_user_id, name, type, equity_role,
            asset_role, liability_type, liability_direction, interest_rate, interest_period,
            currency_code, iban, bic, account_number, virtual_balance_minor, include_net_worth,
            active, sort_order, notes, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.household_id,
        row.owner_user_id,
        row.name,
        row.account_type,
        row.equity_role,
        row.asset_role,
        row.liability_type,
        row.liability_direction,
        row.interest_rate,
        row.interest_period,
        row.currency_code,
        row.iban,
        row.bic,
        row.account_number,
        row.virtual_balance_minor,
        row.include_net_worth,
        row.active,
        row.sort_order,
        row.notes,
        row.created_at,
        row.updated_at,
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| {
        AppError::conflict_on_unique(e, "an account of this type with this name exists")
    })?;
    Ok(())
}

/// One account of the household, or `None`.
pub(crate) async fn get(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<Option<AccountRow>, AppError> {
    let row = sqlx::query_as!(
        AccountRow,
        r#"SELECT id, household_id, owner_user_id, name, type AS account_type, equity_role,
                  asset_role, liability_type, liability_direction, interest_rate, interest_period,
                  currency_code, iban, bic, account_number, virtual_balance_minor,
                  include_net_worth AS "include_net_worth: bool", active AS "active: bool",
                  sort_order, notes, created_at, updated_at
           FROM accounts WHERE household_id = ? AND id = ?"#,
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

/// The household's accounts whose IDs are in `ids` (a JSON array of strings).
pub(crate) async fn get_many(
    conn: &mut SqliteConnection,
    household_id: &str,
    ids_json: &str,
) -> Result<Vec<AccountRow>, AppError> {
    // `json_each` turns the JSON array into rows, so one checked query can
    // take a list of any length.
    let rows = sqlx::query_as!(
        AccountRow,
        r#"SELECT id, household_id, owner_user_id, name, type AS account_type, equity_role,
                  asset_role, liability_type, liability_direction, interest_rate, interest_period,
                  currency_code, iban, bic, account_number, virtual_balance_minor,
                  include_net_worth AS "include_net_worth: bool", active AS "active: bool",
                  sort_order, notes, created_at, updated_at
           FROM accounts
           WHERE household_id = ? AND id IN (SELECT value FROM json_each(?))"#,
        household_id,
        ids_json,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// Filters for [`list`]; `None` means "any".
pub(crate) struct ListFilter {
    pub account_type: Option<String>,
    pub active: Option<bool>,
    pub owner_user_id: Option<String>,
    /// Matched anywhere in the name; SQLite's `LIKE` ignores ASCII case.
    pub q: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

pub(crate) async fn list(
    conn: &mut SqliteConnection,
    household_id: &str,
    filter: &ListFilter,
) -> Result<(Vec<AccountRow>, i64), AppError> {
    let rows = sqlx::query_as!(
        AccountRow,
        r#"SELECT id, household_id, owner_user_id, name, type AS account_type, equity_role,
                  asset_role, liability_type, liability_direction, interest_rate, interest_period,
                  currency_code, iban, bic, account_number, virtual_balance_minor,
                  include_net_worth AS "include_net_worth: bool", active AS "active: bool",
                  sort_order, notes, created_at, updated_at
           FROM accounts
           WHERE household_id = ?1 AND (?2 IS NULL OR type = ?2) AND (?3 IS NULL OR active = ?3)
             AND (?4 IS NULL OR owner_user_id = ?4)
             AND (?5 IS NULL OR name LIKE '%' || ?5 || '%')
           ORDER BY type, sort_order, name
           LIMIT ?6 OFFSET ?7"#,
        household_id,
        filter.account_type,
        filter.active,
        filter.owner_user_id,
        filter.q,
        filter.limit,
        filter.offset,
    )
    .fetch_all(&mut *conn)
    .await?;
    let total = sqlx::query_scalar!(
        "SELECT count(*) FROM accounts
         WHERE household_id = ?1 AND (?2 IS NULL OR type = ?2) AND (?3 IS NULL OR active = ?3)
           AND (?4 IS NULL OR owner_user_id = ?4)
           AND (?5 IS NULL OR name LIKE '%' || ?5 || '%')",
        household_id,
        filter.account_type,
        filter.active,
        filter.owner_user_id,
        filter.q,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok((rows, total))
}

/// Writes every changeable column of an existing row.
pub(crate) async fn update(conn: &mut SqliteConnection, row: &AccountRow) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE accounts SET owner_user_id = ?, name = ?, asset_role = ?, liability_type = ?,
            liability_direction = ?, interest_rate = ?, interest_period = ?, iban = ?, bic = ?,
            account_number = ?, virtual_balance_minor = ?, include_net_worth = ?, active = ?,
            sort_order = ?, notes = ?, updated_at = ?
         WHERE household_id = ? AND id = ?",
        row.owner_user_id,
        row.name,
        row.asset_role,
        row.liability_type,
        row.liability_direction,
        row.interest_rate,
        row.interest_period,
        row.iban,
        row.bic,
        row.account_number,
        row.virtual_balance_minor,
        row.include_net_worth,
        row.active,
        row.sort_order,
        row.notes,
        row.updated_at,
        row.household_id,
        row.id,
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| {
        AppError::conflict_on_unique(e, "an account of this type with this name exists")
    })?;
    Ok(())
}

pub(crate) async fn delete(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM accounts WHERE household_id = ? AND id = ?",
        household_id,
        id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn has_splits(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<bool, AppError> {
    let found = sqlx::query_scalar!(
        "SELECT EXISTS (SELECT 1 FROM splits WHERE household_id = ?1
                        AND (source_account_id = ?2 OR destination_account_id = ?2))",
        household_id,
        id,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(found != 0)
}

/// Balance in minor units up to and including `date` (all time if `None`):
/// incoming minus outgoing, using the foreign amount when the account is in
/// the split's foreign currency (ARCHITECTURE.md 4.2).
pub(crate) async fn balance_minor(
    conn: &mut SqliteConnection,
    household_id: &str,
    account: &AccountRow,
    date: Option<&str>,
) -> Result<i64, AppError> {
    let balance = sqlx::query_scalar!(
        r#"SELECT COALESCE(SUM(
                CASE WHEN destination_account_id = ?2 THEN moved ELSE -moved END), 0) AS "balance!: i64"
           FROM (SELECT source_account_id, destination_account_id,
                        CASE WHEN foreign_currency_code = ?3 THEN foreign_amount_minor
                             ELSE amount_minor END AS moved
                 FROM splits
                 WHERE household_id = ?1
                   AND (source_account_id = ?2 OR destination_account_id = ?2)
                   AND (?4 IS NULL OR date <= ?4))"#,
        household_id,
        account.id,
        account.currency_code,
        date,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(balance)
}
