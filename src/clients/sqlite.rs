// SQLite connection setup: opens (creating if missing) a connection pool
// for the given `sqlx` connection string, then ensures the schema — two
// tables, `templates` and `documents` — is present. No migration framework:
// the schema is small and stable enough that two `CREATE TABLE IF NOT
// EXISTS` statements are simpler than a migrations directory, and every
// column change so far has been additive.

use std::str::FromStr;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

pub async fn connect(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        // WAL allows concurrent readers alongside a writer, which matters
        // here since `SqlitePoolOptions` hands out more than one connection
        // — the default rollback-journal mode serializes all access and
        // would otherwise surface as spurious "database is locked" errors
        // under concurrent requests.
        .journal_mode(SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    create_schema(&pool).await?;

    Ok(pool)
}

async fn create_schema(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS templates (
            key TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            type TEXT NOT NULL DEFAULT 'document',
            description TEXT NOT NULL DEFAULT '',
            data_schema TEXT,
            is_active INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    // In case an existing database created before the `type` column was added is being reused
    let _ = sqlx::query("ALTER TABLE templates ADD COLUMN type TEXT NOT NULL DEFAULT 'document'")
        .execute(pool)
        .await;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS documents (
            key TEXT PRIMARY KEY,
            template_key TEXT NOT NULL,
            data TEXT NOT NULL,
            file_size_bytes INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
