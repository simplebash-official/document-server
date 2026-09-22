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
    let options = with_statement_logging(SqliteConnectOptions::from_str(database_url)?)
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

/// Wires sqlx statement logging unless the process started with
/// `LOG_SQL=off`. Which statements actually reach the log is then decided per
/// event by the runtime filter in `core::logging::init`, so a `log_mode`
/// control line can switch between every statement, slow-only and none
/// without reconnecting the pool.
fn with_statement_logging(options: SqliteConnectOptions) -> SqliteConnectOptions {
    use crate::core::logging::{SqlLogging, settings};
    use sqlx::ConnectOptions;
    if settings().sql_at_startup == SqlLogging::Off {
        return options.disable_statement_logging();
    }
    options
        .log_statements(log::LevelFilter::Info)
        .log_slow_statements(
            log::LevelFilter::Warn,
            std::time::Duration::from_millis(250),
        )
}

async fn create_schema(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS templates (
            key TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            type TEXT NOT NULL DEFAULT 'document',
            description TEXT NOT NULL DEFAULT '',
            data_schema TEXT,
            sample_data TEXT,
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

    // Same additive pattern as `type` above. `data_schema` predates this and
    // historically held whatever `<name>.json` sidecar contained (sample
    // data) under a misleading name; from the schema-sidecar feature onward
    // it holds the real `<name>.schema.json` contract instead, and samples
    // move to their own honestly-named column. The next disk sync rewrites
    // both columns, so an old database self-corrects on first boot.
    let _ = sqlx::query("ALTER TABLE templates ADD COLUMN sample_data TEXT")
        .execute(pool)
        .await;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS documents (
            key TEXT PRIMARY KEY,
            tenant_key TEXT NOT NULL DEFAULT '',
            template_key TEXT NOT NULL,
            data TEXT NOT NULL,
            file_size_bytes INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS template_data (
            key TEXT PRIMARY KEY,
            tenant_key TEXT NOT NULL DEFAULT '',
            template_name TEXT NOT NULL,
            data_key TEXT NOT NULL,
            data TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    // Tenant scoping for the shared multi-tenant cloud deployment (one
    // document-server instance behind every tenant of the cloud POS backend).
    // `tenant_key = ''` is the single-shop/desktop sentinel — that deployment
    // never sends `X-Tenant-Key` (see `core::middleware::auth`), so its rows
    // stay under the one implicit tenant they always were. Additive
    // `ALTER TABLE` for a database created before this column existed,
    // exactly like `type`/`sample_data` above; the error from a database
    // that already has the column (freshly created via the literal above) is
    // expected and ignored.
    let _ = sqlx::query("ALTER TABLE documents ADD COLUMN tenant_key TEXT NOT NULL DEFAULT ''")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE template_data ADD COLUMN tenant_key TEXT NOT NULL DEFAULT ''")
        .execute(pool)
        .await;

    // The ADD COLUMN above does NOT remove the OLD inline
    // `UNIQUE (template_name, data_key)` constraint on a database created
    // before tenant scoping existed — SQLite has no `ALTER TABLE DROP
    // CONSTRAINT`, and that constraint is silently kept in force. Left as-is
    // it's actively wrong, not just redundant: it would still reject a
    // second tenant storing under a `(template_name, data_key)` pair another
    // tenant already used, i.e. exactly the collision tenant scoping exists
    // to prevent. Detect it via `sqlite_master`'s stored `CREATE TABLE` text
    // (`ALTER TABLE ADD COLUMN` preserves the original clauses verbatim
    // elsewhere in that text) and rebuild the table without it — SQLite's
    // standard create/copy/drop/rename dance, the only way to change a
    // table's constraints. Idempotent: a database this has already run
    // against (or one created fresh after tenant scoping shipped) has no
    // such text to find, so this is a no-op there.
    if table_has_old_narrow_unique_constraint(pool, "template_data").await? {
        let mut tx = pool.begin().await?;
        sqlx::query(
            "CREATE TABLE template_data_tenant_scoped (
                key TEXT PRIMARY KEY,
                tenant_key TEXT NOT NULL DEFAULT '',
                template_name TEXT NOT NULL,
                data_key TEXT NOT NULL,
                data TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO template_data_tenant_scoped
             SELECT key, tenant_key, template_name, data_key, data, created_at, updated_at
             FROM template_data",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query("DROP TABLE template_data")
            .execute(&mut *tx)
            .await?;
        sqlx::query("ALTER TABLE template_data_tenant_scoped RENAME TO template_data")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
    }

    // A named index so the `upsert` repository query's
    // `ON CONFLICT(tenant_key, template_name, data_key)` has a stable target
    // on every database — freshly created (no inline constraint at all) or
    // just rebuilt above (same reason).
    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_template_data_scope
         ON template_data (tenant_key, template_name, data_key)",
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Whether `table`'s original `CREATE TABLE` text (as SQLite still stores it
/// in `sqlite_master`, `ALTER TABLE ADD COLUMN` notwithstanding) contains the
/// pre-tenant-scoping inline `UNIQUE (template_name, data_key)` clause. The
/// exact literal this checks for is the one dropped from the `CREATE TABLE`
/// call above — keep the two in sync if that literal's spacing ever changes.
async fn table_has_old_narrow_unique_constraint(
    pool: &SqlitePool,
    table: &str,
) -> Result<bool, sqlx::Error> {
    let sql: Option<String> =
        sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_optional(pool)
            .await?;
    Ok(sql.is_some_and(|s| s.contains("UNIQUE (template_name, data_key)")))
}
