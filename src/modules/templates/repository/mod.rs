// SQLite access for the `templates` and `template_grants` tables only.
// Functions here never interpret a missing row as an error — they return
// `Option`/`Vec` straight from the driver and leave the "not found" ->
// `AppError` translation to `service`. Visibility is `pub(crate)` so
// `service` can call in, but the `mod repository;` declaration in
// `templates/mod.rs` is private, so none of this is reachable from outside
// the `templates` module tree.

use chrono::Utc;
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::{core::error::AppResult, modules::templates::model::TemplateRow};

/// A published template the tenant may render: every global one, plus the
/// private ones whose family was granted to the tenant.
const RENDERABLE: &str = "t.status = 'published' AND (t.scope = 'global' OR EXISTS (
    SELECT 1 FROM template_grants g WHERE g.family = t.family AND g.tenant_key = ?))";

/// A template the tenant may reprint history of: same as `RENDERABLE`
/// without the status filter, so a retired design still reprints.
const REPRINTABLE: &str = "(t.scope = 'global' OR EXISTS (
    SELECT 1 FROM template_grants g WHERE g.family = t.family AND g.tenant_key = ?))";

pub(crate) async fn find_renderable_by_key(
    db: &SqlitePool,
    tenant_key: &str,
    key: &str,
) -> AppResult<Option<TemplateRow>> {
    let sql = format!("SELECT t.* FROM templates t WHERE t.key = ? AND {RENDERABLE}");
    // `AssertSqlSafe` is justified: the interpolated text is a compile-time
    // constant; every value travels as a bind parameter.
    Ok(sqlx::query_as::<_, TemplateRow>(sqlx::AssertSqlSafe(sql))
        .bind(key)
        .bind(tenant_key)
        .fetch_optional(db)
        .await?)
}

pub(crate) async fn find_reprintable_by_key(
    db: &SqlitePool,
    tenant_key: &str,
    key: &str,
) -> AppResult<Option<TemplateRow>> {
    let sql = format!("SELECT t.* FROM templates t WHERE t.key = ? AND {REPRINTABLE}");
    Ok(sqlx::query_as::<_, TemplateRow>(sqlx::AssertSqlSafe(sql))
        .bind(key)
        .bind(tenant_key)
        .fetch_optional(db)
        .await?)
}

/// Lookup by *name* with no visibility or status filter — for the admin
/// package routes and for tenant-scoped data keyed by name.
pub(crate) async fn find_by_name(db: &SqlitePool, name: &str) -> AppResult<Option<TemplateRow>> {
    Ok(
        sqlx::query_as::<_, TemplateRow>("SELECT * FROM templates WHERE name = ?")
            .bind(name)
            .fetch_optional(db)
            .await?,
    )
}

/// Latest published version of each family the tenant may render, optionally
/// filtered by type, ordered by title then family.
pub(crate) async fn list_renderable_latest(
    db: &SqlitePool,
    tenant_key: &str,
    type_filter: Option<&str>,
) -> AppResult<Vec<TemplateRow>> {
    let sql = format!(
        "SELECT t.* FROM templates t
         WHERE {RENDERABLE}
           AND t.version = (
               SELECT MAX(l.version) FROM templates l WHERE l.family = t.family AND l.status = 'published')
           AND (? IS NULL OR t.type = ?)
         ORDER BY t.description ASC, t.family ASC"
    );
    Ok(sqlx::query_as::<_, TemplateRow>(sqlx::AssertSqlSafe(sql))
        .bind(tenant_key)
        .bind(type_filter)
        .bind(type_filter)
        .fetch_all(db)
        .await?)
}

/// Every row (all statuses and scopes, all versions) — admin listing.
pub(crate) async fn list_all(db: &SqlitePool) -> AppResult<Vec<TemplateRow>> {
    Ok(sqlx::query_as::<_, TemplateRow>(
        "SELECT * FROM templates ORDER BY family ASC, version DESC",
    )
    .fetch_all(db)
    .await?)
}

pub(crate) async fn max_version(db: &SqlitePool, family: &str) -> AppResult<Option<i64>> {
    Ok(
        sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(version) FROM templates WHERE family = ?")
            .bind(family)
            .fetch_one(db)
            .await?,
    )
}

/// Another family already uses `title` (versions of one family may share it).
pub(crate) async fn title_used_by_other_family(
    db: &SqlitePool,
    title: &str,
    family: &str,
) -> AppResult<bool> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM templates WHERE description = ? AND family != ?")
            .bind(title)
            .bind(family)
            .fetch_one(db)
            .await?;
    Ok(count > 0)
}

/// Inserts or refreshes a bundled (global) template by name. The row's `key`,
/// `family` and `version` are fixed at first insert; everything the files on
/// disk define is refreshed, and the template is (re)published.
pub(crate) async fn upsert_global(db: &SqlitePool, row: &TemplateRow) -> AppResult<TemplateRow> {
    let now = Utc::now();
    Ok(sqlx::query_as::<_, TemplateRow>(
        "INSERT INTO templates
            (key, name, family, version, type, kind, description, data_schema, sample_data,
             status, scope, file_path, sha256, min_app_version, created_at, updated_at)
         VALUES (?, ?, ?, 1, ?, ?, ?, ?, ?, 'published', 'global', ?, ?, NULL, ?, ?)
         ON CONFLICT(name) DO UPDATE SET
            type = excluded.type, kind = excluded.kind, description = excluded.description,
            data_schema = excluded.data_schema, sample_data = excluded.sample_data,
            status = 'published', scope = 'global', file_path = excluded.file_path,
            sha256 = excluded.sha256, updated_at = excluded.updated_at
         RETURNING *",
    )
    .bind(&row.key)
    .bind(&row.name)
    .bind(&row.family)
    .bind(&row.r#type)
    .bind(&row.kind)
    .bind(&row.description)
    .bind(&row.data_schema)
    .bind(&row.sample_data)
    .bind(&row.file_path)
    .bind(&row.sha256)
    .bind(now)
    .bind(now)
    .fetch_one(db)
    .await?)
}

/// Retires every global template whose name is not in `names` — bundled
/// templates removed from the shipped set. Rows are never deleted: documents
/// reference them.
pub(crate) async fn retire_globals_not_in(db: &SqlitePool, names: &[String]) -> AppResult<u64> {
    let now = Utc::now();
    if names.is_empty() {
        let result = sqlx::query(
            "UPDATE templates SET status = 'retired', updated_at = ?
             WHERE scope = 'global' AND status = 'published'",
        )
        .bind(now)
        .execute(db)
        .await?;
        return Ok(result.rows_affected());
    }

    let placeholders = vec!["?"; names.len()].join(", ");
    let sql = format!(
        "UPDATE templates SET status = 'retired', updated_at = ?
         WHERE scope = 'global' AND status = 'published' AND name NOT IN ({placeholders})"
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(now);
    for name in names {
        query = query.bind(name);
    }
    Ok(query.execute(db).await?.rows_affected())
}

pub(crate) async fn insert_private(
    tx: &mut Transaction<'_, Sqlite>,
    row: &TemplateRow,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO templates
            (key, name, family, version, type, kind, description, data_schema, sample_data,
             status, scope, file_path, sha256, min_app_version, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'private', ?, ?, ?, ?, ?)",
    )
    .bind(&row.key)
    .bind(&row.name)
    .bind(&row.family)
    .bind(row.version)
    .bind(&row.r#type)
    .bind(&row.kind)
    .bind(&row.description)
    .bind(&row.data_schema)
    .bind(&row.sample_data)
    .bind(&row.status)
    .bind(&row.file_path)
    .bind(&row.sha256)
    .bind(&row.min_app_version)
    .bind(row.created_at)
    .bind(row.updated_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn retire_family(db: &SqlitePool, family: &str) -> AppResult<u64> {
    let result = sqlx::query(
        "UPDATE templates SET status = 'retired', updated_at = ?
         WHERE family = ? AND scope = 'private' AND status = 'published'",
    )
    .bind(Utc::now())
    .bind(family)
    .execute(db)
    .await?;
    Ok(result.rows_affected())
}

/// Replaces the set of tenants a private family is granted to.
pub(crate) async fn replace_grants(
    db: &SqlitePool,
    family: &str,
    tenant_keys: &[String],
) -> AppResult<()> {
    let now = Utc::now();
    let mut tx = db.begin().await?;
    sqlx::query("DELETE FROM template_grants WHERE family = ?")
        .bind(family)
        .execute(&mut *tx)
        .await?;
    for tenant_key in tenant_keys {
        sqlx::query(
            "INSERT INTO template_grants (family, tenant_key, created_at) VALUES (?, ?, ?)",
        )
        .bind(family)
        .bind(tenant_key)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub(crate) async fn add_grant(db: &SqlitePool, family: &str, tenant_key: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO template_grants (family, tenant_key, created_at) VALUES (?, ?, ?)
         ON CONFLICT(family, tenant_key) DO NOTHING",
    )
    .bind(family)
    .bind(tenant_key)
    .bind(Utc::now())
    .execute(db)
    .await?;
    Ok(())
}

/// `(family, tenant_key)` pairs for every granted family.
pub(crate) async fn list_grants(db: &SqlitePool) -> AppResult<Vec<(String, String)>> {
    Ok(sqlx::query_as::<_, (String, String)>(
        "SELECT family, tenant_key FROM template_grants ORDER BY family, tenant_key",
    )
    .fetch_all(db)
    .await?)
}

pub(crate) async fn is_granted(db: &SqlitePool, family: &str, tenant_key: &str) -> AppResult<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM template_grants WHERE family = ? AND tenant_key = ?",
    )
    .bind(family)
    .bind(tenant_key)
    .fetch_one(db)
    .await?;
    Ok(count > 0)
}

pub(crate) async fn find_family_head(
    db: &SqlitePool,
    family: &str,
) -> AppResult<Option<TemplateRow>> {
    Ok(sqlx::query_as::<_, TemplateRow>(
        "SELECT * FROM templates WHERE family = ? ORDER BY version DESC LIMIT 1",
    )
    .bind(family)
    .fetch_optional(db)
    .await?)
}
