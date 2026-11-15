//! SQL for `transaction_groups`, `splits` and `split_tags`.

use saldo_core::{SplitId, SplitKind, ValidatedGroup};
use sqlx::SqliteConnection;

use crate::clock::{Clock, new_id};
use crate::error::AppError;

pub(crate) fn kind_name(kind: SplitKind) -> &'static str {
    match kind {
        SplitKind::Withdrawal => "withdrawal",
        SplitKind::Deposit => "deposit",
        SplitKind::Transfer => "transfer",
        SplitKind::OpeningBalance => "opening_balance",
        SplitKind::Reconciliation => "reconciliation",
    }
}

pub(crate) async fn insert_group(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
    title: Option<&str>,
    created_by: &str,
    now: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO transaction_groups (id, household_id, title, created_by_user_id, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)",
        id, household_id, title, created_by, now, now,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn update_group(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
    title: Option<&str>,
    now: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE transaction_groups SET title = ?, updated_at = ? WHERE household_id = ? AND id = ?",
        title,
        now,
        household_id,
        id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn group_exists(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<bool, AppError> {
    let found = sqlx::query_scalar!(
        "SELECT EXISTS (SELECT 1 FROM transaction_groups WHERE household_id = ? AND id = ?)",
        household_id,
        id,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(found != 0)
}

/// Deletes the group; its splits and their tags go with it (ON DELETE CASCADE).
pub(crate) async fn delete_group(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM transaction_groups WHERE household_id = ? AND id = ?",
        household_id,
        id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The IDs of the group's splits, in position order.
pub(crate) async fn split_ids(
    conn: &mut SqliteConnection,
    household_id: &str,
    group_id: &str,
) -> Result<Vec<String>, AppError> {
    let ids = sqlx::query_scalar!(
        "SELECT id FROM splits WHERE household_id = ? AND group_id = ? ORDER BY position",
        household_id,
        group_id
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(ids)
}

/// Deletes one split of a group (its tags go with it).
pub(crate) async fn delete_split(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM splits WHERE household_id = ? AND id = ?",
        household_id,
        id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Moves the group's splits to negative positions, so [`write_splits`] can
/// give them their new positions without `UNIQUE (group_id, position)`
/// tripping over a split that has not moved yet.
pub(crate) async fn park_positions(
    conn: &mut SqliteConnection,
    household_id: &str,
    group_id: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE splits SET position = -1 - position WHERE household_id = ? AND group_id = ?",
        household_id,
        group_id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Writes every split of `group` and its tags. `ids[n]` is the existing
/// split that split `n` continues: it is updated in place (an upsert, so
/// rows pointing at it keep pointing at it); a missing ID makes a new split.
pub(crate) async fn write_splits(
    conn: &mut SqliteConnection,
    clock: &dyn Clock,
    household_id: &str,
    group_id: &str,
    group: &ValidatedGroup,
    ids: &[Option<SplitId>],
    now: &str,
) -> Result<(), AppError> {
    for (position, validated) in group.splits().iter().enumerate() {
        let split = validated.split();
        let id = match ids.get(position).copied().flatten() {
            Some(existing) => existing.to_string(),
            None => new_id(clock).to_string(),
        };
        let position = i64::try_from(position)
            .map_err(|_| AppError::Internal("too many splits".to_string()))?;
        let kind = kind_name(validated.kind());
        let date = split.date.to_string();
        let (source, destination) = (split.source.to_string(), split.destination.to_string());
        let (amount, currency) = (split.amount.minor(), split.amount.currency().to_string());
        let foreign_amount = split.foreign_amount.map(|m| m.minor());
        let foreign_currency = split.foreign_amount.map(|m| m.currency().to_string());
        let category = split.category.map(|c| c.to_string());
        let budget = split.budget.map(|b| b.to_string());
        let bill = split.bill.map(|b| b.to_string());
        let meta = serde_json::to_string(&split.meta)
            .map_err(|e| AppError::Internal(format!("cannot encode meta: {e}")))?;
        sqlx::query!(
            "INSERT INTO splits (id, household_id, group_id, position, kind, date, description,
                source_account_id, destination_account_id, amount_minor, currency_code,
                foreign_amount_minor, foreign_currency_code, category_id, budget_id, bill_id,
                reconciled, notes, meta, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (id) DO UPDATE SET position = excluded.position, kind = excluded.kind,
                date = excluded.date, description = excluded.description,
                source_account_id = excluded.source_account_id,
                destination_account_id = excluded.destination_account_id,
                amount_minor = excluded.amount_minor, currency_code = excluded.currency_code,
                foreign_amount_minor = excluded.foreign_amount_minor,
                foreign_currency_code = excluded.foreign_currency_code,
                category_id = excluded.category_id, budget_id = excluded.budget_id,
                bill_id = excluded.bill_id, reconciled = excluded.reconciled,
                notes = excluded.notes, meta = excluded.meta, updated_at = excluded.updated_at",
            id,
            household_id,
            group_id,
            position,
            kind,
            date,
            split.description,
            source,
            destination,
            amount,
            currency,
            foreign_amount,
            foreign_currency,
            category,
            budget,
            bill,
            split.reconciled,
            split.notes,
            meta,
            now,
            now,
        )
        .execute(&mut *conn)
        .await?;
        sqlx::query!("DELETE FROM split_tags WHERE split_id = ?", id)
            .execute(&mut *conn)
            .await?;
        for tag in &split.tags {
            let tag = tag.to_string();
            sqlx::query!(
                "INSERT INTO split_tags (split_id, tag_id) VALUES (?, ?)",
                id,
                tag
            )
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}

/// Filters for [`list_ids`] in their stored form; `None` means "any".
#[derive(Debug, Default)]
pub(crate) struct ListFilter {
    pub from: Option<String>,
    pub to: Option<String>,
    pub account_id: Option<String>,
    pub category_id: Option<String>,
    pub tag_id: Option<String>,
    pub budget_id: Option<String>,
    pub bill_id: Option<String>,
    pub kind: Option<String>,
    pub q: Option<String>,
    pub owner_user_id: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

/// IDs of the groups with at least one matching split, newest split date
/// first, plus the total number of matches.
pub(crate) async fn list_ids(
    conn: &mut SqliteConnection,
    household_id: &str,
    f: &ListFilter,
) -> Result<(Vec<String>, i64), AppError> {
    // `?n IS NULL OR ...` makes every filter optional in one checked query.
    let ids = sqlx::query_scalar!(
        r#"SELECT g.id AS "id!" FROM transaction_groups g
           WHERE g.household_id = ?1 AND EXISTS (
             SELECT 1 FROM splits s WHERE s.group_id = g.id
               AND (?2 IS NULL OR s.date >= ?2) AND (?3 IS NULL OR s.date <= ?3)
               AND (?4 IS NULL OR s.source_account_id = ?4 OR s.destination_account_id = ?4)
               AND (?5 IS NULL OR s.category_id = ?5)
               AND (?6 IS NULL OR EXISTS (SELECT 1 FROM split_tags st WHERE st.split_id = s.id AND st.tag_id = ?6))
               AND (?7 IS NULL OR s.budget_id = ?7) AND (?8 IS NULL OR s.bill_id = ?8)
               AND (?9 IS NULL OR s.kind = ?9)
               AND (?10 IS NULL OR s.description LIKE '%' || ?10 || '%' OR g.title LIKE '%' || ?10 || '%')
               AND (?11 IS NULL OR EXISTS (SELECT 1 FROM accounts a WHERE a.household_id = g.household_id
                     AND a.id IN (s.source_account_id, s.destination_account_id) AND a.owner_user_id = ?11)))
           ORDER BY (SELECT max(s2.date) FROM splits s2 WHERE s2.group_id = g.id) DESC, g.id DESC
           LIMIT ?12 OFFSET ?13"#,
        household_id, f.from, f.to, f.account_id, f.category_id, f.tag_id, f.budget_id, f.bill_id,
        f.kind, f.q, f.owner_user_id, f.limit, f.offset,
    )
    .fetch_all(&mut *conn)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) FROM transaction_groups g
           WHERE g.household_id = ?1 AND EXISTS (
             SELECT 1 FROM splits s WHERE s.group_id = g.id
               AND (?2 IS NULL OR s.date >= ?2) AND (?3 IS NULL OR s.date <= ?3)
               AND (?4 IS NULL OR s.source_account_id = ?4 OR s.destination_account_id = ?4)
               AND (?5 IS NULL OR s.category_id = ?5)
               AND (?6 IS NULL OR EXISTS (SELECT 1 FROM split_tags st WHERE st.split_id = s.id AND st.tag_id = ?6))
               AND (?7 IS NULL OR s.budget_id = ?7) AND (?8 IS NULL OR s.bill_id = ?8)
               AND (?9 IS NULL OR s.kind = ?9)
               AND (?10 IS NULL OR s.description LIKE '%' || ?10 || '%' OR g.title LIKE '%' || ?10 || '%')
               AND (?11 IS NULL OR EXISTS (SELECT 1 FROM accounts a WHERE a.household_id = g.household_id
                     AND a.id IN (s.source_account_id, s.destination_account_id) AND a.owner_user_id = ?11)))"#,
        household_id, f.from, f.to, f.account_id, f.category_id, f.tag_id, f.budget_id, f.bill_id,
        f.kind, f.q, f.owner_user_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok((ids, total))
}
