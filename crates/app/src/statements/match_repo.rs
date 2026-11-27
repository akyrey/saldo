//! SQL for the source rows explaining an import's lines (Track D).

use sqlx::SqliteConnection;

use crate::error::AppError;

/// Records that a source row explains a draft line (Track D).
#[allow(clippy::too_many_arguments)] // one column each
pub(super) async fn insert_source_match(
    conn: &mut SqliteConnection,
    household_id: &str,
    source_row_id: &str,
    import_id: &str,
    position: i64,
    match_type: &str,
    amount_diff_minor: i64,
    created_at: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO source_matches
            (household_id, source_row_id, statement_import_id, line_position, match_type, amount_diff_minor, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        household_id,
        source_row_id,
        import_id,
        position,
        match_type,
        amount_diff_minor,
        created_at
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// A committed line's matches now explain its split.
pub(super) async fn attach_matches(
    conn: &mut SqliteConnection,
    import_id: &str,
    position: i64,
    split_id: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE source_matches SET split_id = ? WHERE statement_import_id = ? AND line_position = ?",
        split_id,
        import_id,
        position
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Forgets what an import's lines matched (a discard or a revert).
pub(super) async fn drop_matches(
    conn: &mut SqliteConnection,
    import_id: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM source_matches WHERE statement_import_id = ?",
        import_id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Whether any source row still explains this draft line.
pub(super) async fn line_has_matches(
    conn: &mut SqliteConnection,
    import_id: &str,
    position: i64,
) -> Result<bool, AppError> {
    let found = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM source_matches WHERE statement_import_id = ? AND line_position = ?",
        import_id,
        position
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(found > 0)
}

/// Forgets the matches of an import's lines that booked nothing.
pub(super) async fn drop_unattached_matches(
    conn: &mut SqliteConnection,
    import_id: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM source_matches WHERE statement_import_id = ? AND split_id IS NULL",
        import_id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}
