//! SQL for `credit_card_plans`, `credit_card_limits` and `credit_card_moves`,
//! and the card's splits. Every function filters by household.

use sqlx::SqliteConnection;

use crate::error::AppError;

/// A `credit_card_plans` row as stored.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlanRow {
    pub account_id: String,
    pub settlement_account_id: String,
    pub settlement_day: i64,
    pub starts_cycle: String,
    pub created_at: String,
    pub updated_at: String,
}

/// The fields of an account the plan rules look at.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AccountFacts {
    pub id: String,
    pub name: String,
    pub account_type: String,
    pub asset_role: Option<String>,
    pub currency_code: String,
    pub active: bool,
}

/// One split that counts toward a card (decision 6): `amount_minor` is
/// positive for a purchase, negative for a refund.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CardSplitRow {
    pub id: String,
    pub group_id: String,
    pub date: String,
    pub description: String,
    pub amount_minor: i64,
    pub counterparty_id: String,
    pub counterparty_name: String,
    pub moved: bool,
}

pub(crate) async fn account(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<Option<AccountFacts>, AppError> {
    let row = sqlx::query_as!(
        AccountFacts,
        r#"SELECT id AS "id!", name, type AS account_type, asset_role, currency_code,
                  active AS "active: bool"
           FROM accounts WHERE household_id = ? AND id = ?"#,
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

pub(crate) async fn plan(
    conn: &mut SqliteConnection,
    household_id: &str,
    account_id: &str,
) -> Result<Option<PlanRow>, AppError> {
    let row = sqlx::query_as!(
        PlanRow,
        r#"SELECT account_id AS "account_id!", settlement_account_id, settlement_day,
                  starts_cycle, created_at, updated_at
           FROM credit_card_plans WHERE household_id = ? AND account_id = ?"#,
        household_id,
        account_id,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

/// Every plan of the household, by card name.
pub(crate) async fn plans(
    conn: &mut SqliteConnection,
    household_id: &str,
) -> Result<Vec<PlanRow>, AppError> {
    let rows = sqlx::query_as!(
        PlanRow,
        r#"SELECT p.account_id AS "account_id!", p.settlement_account_id, p.settlement_day,
                  p.starts_cycle, p.created_at, p.updated_at
           FROM credit_card_plans p
           JOIN accounts a ON a.household_id = p.household_id AND a.id = p.account_id
           WHERE p.household_id = ?
           ORDER BY a.sort_order, a.name"#,
        household_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

pub(crate) async fn insert_plan(
    conn: &mut SqliteConnection,
    household_id: &str,
    row: &PlanRow,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO credit_card_plans (account_id, household_id, settlement_account_id,
            settlement_day, starts_cycle, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        row.account_id,
        household_id,
        row.settlement_account_id,
        row.settlement_day,
        row.starts_cycle,
        row.created_at,
        row.updated_at,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn update_plan(
    conn: &mut SqliteConnection,
    household_id: &str,
    row: &PlanRow,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE credit_card_plans SET settlement_account_id = ?, settlement_day = ?, updated_at = ?
         WHERE household_id = ? AND account_id = ?",
        row.settlement_account_id,
        row.settlement_day,
        row.updated_at,
        household_id,
        row.account_id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Deletes the plan; its limits, moves and month rows go with it (cascade).
pub(crate) async fn delete_plan(
    conn: &mut SqliteConnection,
    household_id: &str,
    account_id: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM credit_card_plans WHERE household_id = ? AND account_id = ?",
        household_id,
        account_id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// `(is a card with a plan, pays a card's plan)` for an account.
pub(crate) async fn plan_roles(
    conn: &mut SqliteConnection,
    household_id: &str,
    account_id: &str,
) -> Result<(bool, bool), AppError> {
    let row = sqlx::query!(
        r#"SELECT
             EXISTS (SELECT 1 FROM credit_card_plans
                     WHERE household_id = ?1 AND account_id = ?2) AS "is_card!: bool",
             EXISTS (SELECT 1 FROM credit_card_plans
                     WHERE household_id = ?1 AND settlement_account_id = ?2) AS "pays!: bool""#,
        household_id,
        account_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok((row.is_card, row.pays))
}

/// The plan's budgets as `(from_cycle, limit_minor)`, oldest first.
pub(crate) async fn limits(
    conn: &mut SqliteConnection,
    household_id: &str,
    account_id: &str,
) -> Result<Vec<(String, i64)>, AppError> {
    let rows = sqlx::query!(
        "SELECT from_cycle, limit_minor FROM credit_card_limits
         WHERE household_id = ? AND account_id = ? ORDER BY from_cycle",
        household_id,
        account_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.from_cycle, r.limit_minor))
        .collect())
}

/// Sets the budget from `from_cycle` on, replacing an entry for that month.
pub(crate) async fn upsert_limit(
    conn: &mut SqliteConnection,
    household_id: &str,
    account_id: &str,
    from_cycle: &str,
    limit_minor: i64,
    now: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO credit_card_limits (household_id, account_id, from_cycle, limit_minor, created_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (account_id, from_cycle) DO UPDATE SET limit_minor = excluded.limit_minor",
        household_id,
        account_id,
        from_cycle,
        limit_minor,
        now,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The card's counting splits dated `from` to `to` (inclusive), oldest
/// first, each with whether it was moved to the next month.
pub(crate) async fn card_splits(
    conn: &mut SqliteConnection,
    household_id: &str,
    card_id: &str,
    from: &str,
    to: &str,
) -> Result<Vec<CardSplitRow>, AppError> {
    let rows = sqlx::query_as!(
        CardSplitRow,
        r#"SELECT s.id AS "id!", s.group_id AS "group_id!", s.date AS "date!",
                  s.description AS "description!",
                  CASE WHEN s.kind = 'withdrawal' THEN s.amount_minor ELSE -s.amount_minor END
                      AS "amount_minor!: i64",
                  c.id AS "counterparty_id!", c.name AS "counterparty_name!",
                  (m.split_id IS NOT NULL) AS "moved!: bool"
           FROM splits s
           JOIN accounts c ON c.household_id = s.household_id
                AND c.id = CASE WHEN s.kind = 'withdrawal'
                                THEN s.destination_account_id ELSE s.source_account_id END
           LEFT JOIN credit_card_moves m ON m.split_id = s.id
                AND m.household_id = s.household_id AND m.account_id = ?3
           WHERE s.household_id = ?1 AND s.date >= ?4 AND s.date <= ?5
             AND ((s.kind = 'withdrawal' AND s.source_account_id = ?2)
                  OR (s.kind = 'deposit' AND s.destination_account_id = ?2))
           ORDER BY s.date, s.id"#,
        household_id,
        card_id,
        card_id,
        from,
        to,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// One split of the household if it counts toward the card.
pub(crate) async fn card_split(
    conn: &mut SqliteConnection,
    household_id: &str,
    card_id: &str,
    split_id: &str,
) -> Result<Option<CardSplitRow>, AppError> {
    let row = sqlx::query_as!(
        CardSplitRow,
        r#"SELECT s.id AS "id!", s.group_id AS "group_id!", s.date AS "date!",
                  s.description AS "description!",
                  CASE WHEN s.kind = 'withdrawal' THEN s.amount_minor ELSE -s.amount_minor END
                      AS "amount_minor!: i64",
                  c.id AS "counterparty_id!", c.name AS "counterparty_name!",
                  (m.split_id IS NOT NULL) AS "moved!: bool"
           FROM splits s
           JOIN accounts c ON c.household_id = s.household_id
                AND c.id = CASE WHEN s.kind = 'withdrawal'
                                THEN s.destination_account_id ELSE s.source_account_id END
           LEFT JOIN credit_card_moves m ON m.split_id = s.id
                AND m.household_id = s.household_id AND m.account_id = ?2
           WHERE s.household_id = ?1 AND s.id = ?3
             AND ((s.kind = 'withdrawal' AND s.source_account_id = ?2)
                  OR (s.kind = 'deposit' AND s.destination_account_id = ?2))"#,
        household_id,
        card_id,
        split_id,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

pub(crate) async fn insert_move(
    conn: &mut SqliteConnection,
    household_id: &str,
    card_id: &str,
    split_id: &str,
    now: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO credit_card_moves (split_id, household_id, account_id, created_at)
         VALUES (?, ?, ?, ?)",
        split_id,
        household_id,
        card_id,
        now,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn delete_move(
    conn: &mut SqliteConnection,
    household_id: &str,
    split_id: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM credit_card_moves WHERE household_id = ? AND split_id = ?",
        household_id,
        split_id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Moves whose split no longer counts toward the card (its accounts or kind
/// changed), as `(split_id, date)`.
pub(crate) async fn stale_moves(
    conn: &mut SqliteConnection,
    household_id: &str,
    card_id: &str,
) -> Result<Vec<(String, String)>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT m.split_id AS "split_id!", s.date AS "date!"
           FROM credit_card_moves m
           JOIN splits s ON s.id = m.split_id
           WHERE m.household_id = ?1 AND m.account_id = ?2
             AND NOT ((s.kind = 'withdrawal' AND s.source_account_id = ?2)
                      OR (s.kind = 'deposit' AND s.destination_account_id = ?2))"#,
        household_id,
        card_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(|r| (r.split_id, r.date)).collect())
}

/// Every move of the household as `(split_id, account_id, created_at)`, for
/// the export.
pub(crate) async fn all_moves(
    conn: &mut SqliteConnection,
    household_id: &str,
) -> Result<Vec<(String, String, String)>, AppError> {
    let rows = sqlx::query!(
        "SELECT split_id, account_id, created_at FROM credit_card_moves
         WHERE household_id = ? ORDER BY account_id, created_at, split_id",
        household_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.split_id, r.account_id, r.created_at))
        .collect())
}
