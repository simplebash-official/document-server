// `clients::sqlite::create_schema`'s additive migrations, run directly
// against a hand-built SQLite file (no `common::spawn_app` — these tests
// simulate a database from *before* a given schema change existed, which
// `spawn_app`'s throwaway-file-per-test always starts fresh past).

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;

async fn connect_raw(url: &str) -> sqlx::SqlitePool {
    let options = SqliteConnectOptions::from_str(url)
        .unwrap()
        .create_if_missing(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap()
}

/// A database created before tenant scoping existed keeps the table shape
/// `clients::sqlite::create_schema` used to write, including its inline
/// `UNIQUE (template_name, data_key)` constraint — the constraint
/// `document_server::clients::sqlite::connect` must detect and rebuild the
/// table without (see that function's `table_has_old_narrow_unique_constraint`),
/// since `ALTER TABLE ADD COLUMN` alone cannot remove it and leaving it in
/// place would still reject a second tenant using an already-used
/// `(template_name, data_key)` pair.
#[tokio::test]
async fn upgrading_a_pre_tenant_database_lets_a_second_tenant_reuse_the_same_template_data_key() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let url = format!("sqlite://{}", file.path().display());

    {
        let pool = connect_raw(&url).await;
        sqlx::query(
            "CREATE TABLE template_data (
                key TEXT PRIMARY KEY,
                template_name TEXT NOT NULL,
                data_key TEXT NOT NULL,
                data TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE (template_name, data_key)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO template_data (key, template_name, data_key, data, created_at, updated_at)
             VALUES ('tdat_old1', 'doc_temp_x', 'shop', '{\"a\":1}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;
    }

    let pool = document_server::clients::sqlite::connect(&url)
        .await
        .expect("connecting to a pre-tenant database must migrate it, not fail");

    let tenant_key: String =
        sqlx::query_scalar("SELECT tenant_key FROM template_data WHERE key = 'tdat_old1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        tenant_key, "",
        "a row from before tenant scoping defaults to the single-shop bucket"
    );

    // The real repository upsert query, verbatim: a second tenant storing
    // under the SAME (template_name, data_key) the old row already used.
    sqlx::query(
        "INSERT INTO template_data (key, tenant_key, template_name, data_key, data, created_at, updated_at)
         VALUES ('tdat_tenantb', 'tnt_b', 'doc_temp_x', 'shop', '{\"a\":2}', '2026-01-02T00:00:00Z', '2026-01-02T00:00:00Z')
         ON CONFLICT(tenant_key, template_name, data_key) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at",
    )
    .execute(&pool)
    .await
    .expect("a second tenant must not collide with a pre-existing single-shop row");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM template_data")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2, "both rows exist independently");

    // A same-tenant, same-key upsert still replaces in place, not duplicates
    // — the ordinary single-shop behaviour is unchanged by the rebuild.
    let replaced_key: String = sqlx::query_scalar(
        "INSERT INTO template_data (key, tenant_key, template_name, data_key, data, created_at, updated_at)
         VALUES ('tdat_old1_replacement', '', 'doc_temp_x', 'shop', '{\"a\":3}', '2026-01-03T00:00:00Z', '2026-01-03T00:00:00Z')
         ON CONFLICT(tenant_key, template_name, data_key) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at
         RETURNING key",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        replaced_key, "tdat_old1",
        "the pre-existing single-shop row was updated, not duplicated"
    );

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM template_data")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2);

    pool.close().await;

    // Reconnecting (every real process restart) against an already-migrated
    // database must be a no-op, not fail or touch the data again.
    let pool = document_server::clients::sqlite::connect(&url)
        .await
        .expect("re-running the migration against an already-migrated database must succeed");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM template_data")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 2,
        "no rows lost or duplicated across a second connect"
    );
}

/// `documents` never had a unique constraint to begin with (its only key is
/// the primary key), so its migration is a plain additive `ALTER TABLE` —
/// this is the regression guard for that simpler path.
#[tokio::test]
async fn upgrading_a_pre_tenant_documents_table_defaults_existing_rows_to_the_single_shop_bucket() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let url = format!("sqlite://{}", file.path().display());

    {
        let pool = connect_raw(&url).await;
        sqlx::query(
            "CREATE TABLE documents (
                key TEXT PRIMARY KEY,
                template_key TEXT NOT NULL,
                data TEXT NOT NULL,
                file_size_bytes INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO documents (key, template_key, data, file_size_bytes, created_at, updated_at)
             VALUES ('doc_old1', 'tpl_x', '{}', 100, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;
    }

    let pool = document_server::clients::sqlite::connect(&url)
        .await
        .expect("connecting to a pre-tenant documents table must migrate it, not fail");

    let tenant_key: String =
        sqlx::query_scalar("SELECT tenant_key FROM documents WHERE key = 'doc_old1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(tenant_key, "");
}
