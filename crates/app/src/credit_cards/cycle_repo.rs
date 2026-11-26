//! SQL for `credit_card_cycles` (the settlement bookkeeping) and the
//! settlement groups' splits. Every function filters by household.

use sqlx::SqliteConnection;

use crate::error::AppError;

/// A `credit_card_cycles` row as stored.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CycleRow {
    pub cycle: String,
    pub state: String,
    pub group_id: Option<String>,
    pub written_amount_minor: Option<i64>,
    pub written_date: Option<String>,
    pub written_source_id: Option<String>,
}

/// The only split of a settlement group, as stored.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SettlementSplitRow {
    pub kind: String,
    pub date: String,
    pub source_account_id: String,
    pub destination_account_id: String,
    pub amount_minor: i64,
    pub currency_code: String,
    pub foreign_amount_minor: Option<i64>,
    pub reconciled: bool,
}

pub(crate) async fn cycles(
    conn: &mut SqliteConnection,
    household_id: &str,
    card_id: &str,
) -> Result<Vec<CycleRow>, AppError> {
    let rows = sqlx::query_as!(
        CycleRow,
        r#"SELECT cycle AS "cycle!", state, group_id, written_amount_minor, written_date,
                  written_source_id
           FROM credit_card_cycles WHERE household_id = ? AND account_id = ? ORDER BY cycle"#,
        household_id,
        card_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

pub(crate) async fn upsert_cycle(
    conn: &mut SqliteConnection,
    household_id: &str,
    card_id: &str,
    row: &CycleRow,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO credit_card_cycles (household_id, account_id, cycle, state, group_id,
            written_amount_minor, written_date, written_source_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (account_id, cycle) DO UPDATE SET state = excluded.state,
            group_id = excluded.group_id, written_amount_minor = excluded.written_amount_minor,
            written_date = excluded.written_date, written_source_id = excluded.written_source_id",
        household_id,
        card_id,
        row.cycle,
        row.state,
        row.group_id,
        row.written_amount_minor,
        row.written_date,
        row.written_source_id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The splits of a settlement group (one, unless edited by hand).
pub(crate) async fn settlement_splits(
    conn: &mut SqliteConnection,
    household_id: &str,
    group_id: &str,
) -> Result<Vec<SettlementSplitRow>, AppError> {
    let rows = sqlx::query_as!(
        SettlementSplitRow,
        r#"SELECT kind, date, source_account_id, destination_account_id, amount_minor,
                  currency_code, foreign_amount_minor, reconciled AS "reconciled: bool"
           FROM splits WHERE household_id = ? AND group_id = ? ORDER BY position"#,
        household_id,
        group_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}
