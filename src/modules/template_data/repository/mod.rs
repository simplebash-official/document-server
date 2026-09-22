// SQLite access for the `template_data` table only — no `AppError`, just
// driver results; "not found" → error translation lives in `service`.
// Visibility mirrors every other module: `pub(crate)` fns callable from
// this module tree's service (and nowhere else).
//
// Every query is scoped by `tenant_key` (`""` = single-shop/desktop; see
// `core::middleware::auth::TenantKey`) — the multi-tenant cloud shares one
// document-server instance across every tenant, so this is the only thing
// stopping two tenants' shop-profile/bank-detail blobs from overwriting or
// leaking into each other.

use chrono::Utc;
use sqlx::SqlitePool;

use crate::{core::error::AppResult, modules::template_data::model::TemplateDataRow};

pub(crate) async fn find_by_template_and_key(
    db: &SqlitePool,
    tenant_key: &str,
    template_name: &str,
    data_key: &str,
) -> AppResult<Option<TemplateDataRow>> {
    Ok(sqlx::query_as::<_, TemplateDataRow>(
        "SELECT * FROM template_data WHERE tenant_key = ? AND template_name = ? AND data_key = ?",
    )
    .bind(tenant_key)
    .bind(template_name)
    .bind(data_key)
    .fetch_optional(db)
    .await?)
}

/// Every stored blob for one template *within this tenant*, ordered by
/// `data_key` so the render-time merge is deterministic regardless of
/// insertion order.
pub(crate) async fn list_by_template(
    db: &SqlitePool,
    tenant_key: &str,
    template_name: &str,
) -> AppResult<Vec<TemplateDataRow>> {
    Ok(sqlx::query_as::<_, TemplateDataRow>(
        "SELECT * FROM template_data WHERE tenant_key = ? AND template_name = ? ORDER BY data_key ASC",
    )
    .bind(tenant_key)
    .bind(template_name)
    .fetch_all(db)
    .await?)
}

/// Inserts or replaces the `(tenant_key, template_name, data_key)` blob in
/// one atomic statement — a repeated PUT is an idempotent overwrite, not an
/// error. `ON CONFLICT` targets the named `idx_template_data_scope` index
/// (`clients::sqlite::create_schema`), which exists on every database
/// (freshly created or upgraded-in-place) regardless of the table's original
/// inline `UNIQUE` constraint.
pub(crate) async fn upsert(
    db: &SqlitePool,
    key: String,
    tenant_key: &str,
    template_name: &str,
    data_key: &str,
    data: &serde_json::Value,
) -> AppResult<TemplateDataRow> {
    let now = Utc::now();
    let data_str = data.to_string();

    Ok(sqlx::query_as::<_, TemplateDataRow>(
        "INSERT INTO template_data (key, tenant_key, template_name, data_key, data, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(tenant_key, template_name, data_key) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at
         RETURNING *",
    )
    .bind(key)
    .bind(tenant_key)
    .bind(template_name)
    .bind(data_key)
    .bind(data_str)
    .bind(now)
    .bind(now)
    .fetch_one(db)
    .await?)
}

/// Deletes the `(tenant_key, template_name, data_key)` blob, returning
/// whether a row was actually removed (`service` turns `false` into a 404).
pub(crate) async fn delete_by_template_and_key(
    db: &SqlitePool,
    tenant_key: &str,
    template_name: &str,
    data_key: &str,
) -> AppResult<bool> {
    let result = sqlx::query(
        "DELETE FROM template_data WHERE tenant_key = ? AND template_name = ? AND data_key = ?",
    )
    .bind(tenant_key)
    .bind(template_name)
    .bind(data_key)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}
