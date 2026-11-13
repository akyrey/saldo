//! Database connection pool and migrations.

use std::str::FromStr;
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::migrate::{MigrateError, Migrator};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

/// The migrations in `crates/app/migrations`, embedded into the binary at
/// compile time, so the server needs no SQL files at run time.
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

const MAX_CONNECTIONS: u32 = 8;
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// The settings ARCHITECTURE.md section 5 requires on every connection.
fn with_pragmas(options: SqliteConnectOptions) -> SqliteConnectOptions {
    options
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(BUSY_TIMEOUT)
        .synchronous(SqliteSynchronous::Normal)
}

/// Opens (and creates if missing) the database at `url`, for example
/// `sqlite:///data/saldo.db`.
///
/// `async fn` returns a *future*: work that runs only when someone `.await`s
/// it. Like a JavaScript promise; Go would block a goroutine instead.
pub async fn connect(url: &str) -> Result<SqlitePool, sqlx::Error> {
    let options = with_pragmas(SqliteConnectOptions::from_str(url)?.create_if_missing(true));
    SqlitePoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .connect_with(options)
        .await
}

/// A fresh in-memory database, for tests. An in-memory SQLite database
/// exists per connection, so the pool keeps exactly one connection alive.
pub async fn connect_in_memory() -> Result<SqlitePool, sqlx::Error> {
    let options = with_pragmas(SqliteConnectOptions::from_str("sqlite::memory:")?);
    SqlitePoolOptions::new()
        .min_connections(1)
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect_with(options)
        .await
}

/// A list of IDs as a JSON array, for `... IN (SELECT value FROM json_each(?))`.
pub(crate) fn json_list(ids: &[String]) -> Result<String, crate::error::AppError> {
    serde_json::to_string(ids)
        .map_err(|e| crate::error::AppError::Internal(format!("cannot encode id list: {e}")))
}

/// Writes a consistent copy of the whole database to `path` (`VACUUM INTO`),
/// safe while the server runs. The target file must not exist yet.
pub async fn backup(
    pool: &SqlitePool,
    path: &std::path::Path,
) -> Result<(), crate::error::AppError> {
    if path.exists() {
        return Err(crate::error::AppError::Conflict(format!(
            "{} already exists",
            path.display()
        )));
    }
    // The path goes in as an SQL string literal with its quotes doubled.
    // The existence check below catches a silent no-op (SQLx's in-memory
    // databases, opened as shared-cache URIs, write nothing here).
    let literal = path.to_string_lossy().replace('\'', "''");
    let sql = format!("VACUUM INTO '{literal}'");
    sqlx::query(sqlx::AssertSqlSafe(sql)).execute(pool).await?;
    if !path.exists() {
        return Err(crate::error::AppError::Internal(format!(
            "backup did not create {}",
            path.display()
        )));
    }
    Ok(())
}

/// Applies every migration that has not run yet.
pub async fn migrate(pool: &SqlitePool) -> Result<(), MigrateError> {
    MIGRATOR.run(pool).await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn migrated() -> SqlitePool {
        let pool = connect_in_memory().await.unwrap();
        migrate(&pool).await.unwrap();
        pool
    }

    async fn insert_event(pool: &SqlitePool) {
        sqlx::query(
            "INSERT INTO households (id, name, primary_currency, created_at, updated_at)
             VALUES ('h1', 'Home', 'EUR', 't', 't')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO events (id, household_id, event_type, entity_type, entity_id, origin, payload, occurred_at)
             VALUES ('e1', 'h1', 'test.created', 'test', 'x', 'system', '{}', 't')",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn migrations_apply_and_seed_currencies() {
        let pool = migrated().await;
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM currencies")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(count >= 20);
        // Running them again is a no-op.
        migrate(&pool).await.unwrap();
    }

    #[tokio::test]
    async fn foreign_keys_are_enforced() {
        let pool = migrated().await;
        let result = sqlx::query(
            "INSERT INTO households (id, name, primary_currency, created_at, updated_at)
             VALUES ('h1', 'Home', 'XXX', 't', 't')",
        )
        .execute(&pool)
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn events_cannot_be_updated_or_deleted() {
        let pool = migrated().await;
        insert_event(&pool).await;

        let update = sqlx::query("UPDATE events SET event_type = 'x'")
            .execute(&pool)
            .await;
        let delete = sqlx::query("DELETE FROM events").execute(&pool).await;

        assert!(update.unwrap_err().to_string().contains("append-only"));
        assert!(delete.unwrap_err().to_string().contains("append-only"));
    }

    #[tokio::test]
    async fn backup_copies_the_database_once() {
        // A file database, like production. (SQLx opens in-memory databases as
        // shared-cache URIs, and VACUUM INTO silently writes nothing from those.)
        let dir = std::env::temp_dir().join(format!("saldo-backup-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pool = connect(&format!("sqlite://{}", dir.join("live.db").display()))
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        // A quote in the name checks the escaping.
        let path = dir.join("o'clock.db");
        backup(&pool, &path).await.unwrap();
        let copy = connect(&format!("sqlite://{}", path.display()))
            .await
            .unwrap();
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM currencies")
            .fetch_one(&copy)
            .await
            .unwrap();
        assert!(count > 0);
        assert!(backup(&pool, &path).await.is_err(), "never overwrites");
        copy.close().await;
        pool.close().await;
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn connect_creates_a_database_file() {
        let dir = std::env::temp_dir().join(format!("saldo-db-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("saldo.db");
        let pool = connect(&format!("sqlite://{}", path.display()))
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        let (mode,): (String,) = sqlx::query_as("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(mode, "wal");
        pool.close().await;
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
