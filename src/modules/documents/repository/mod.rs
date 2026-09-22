// SQLite access for the `documents` table only. Functions here never
// interpret a missing row as an error — they return `Option` straight from
// the driver and leave the "not found" -> `AppError` translation to
// `service`. Visibility is `pub(crate)` so `service` can call in, but the
// `mod repository;` declaration in `documents/mod.rs` is private, so none of
// this is reachable from outside the `documents` module tree.

use chrono::Utc;
use sqlx::SqlitePool;
use sqlx::types::Json;

use crate::{core::error::AppResult, modules::documents::model::DocumentRow};

pub(crate) async fn insert_document(
    db: &SqlitePool,
    key: &str,
    tenant_key: &str,
    template_key: &str,
    data: serde_json::Value,
    file_size_bytes: i64,
) -> AppResult<DocumentRow> {
    let now = Utc::now();

    Ok(sqlx::query_as::<_, DocumentRow>(
        "INSERT INTO documents (key, tenant_key, template_key, data, file_size_bytes, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         RETURNING *",
    )
    .bind(key)
    .bind(tenant_key)
    .bind(template_key)
    .bind(Json(data))
    .bind(file_size_bytes)
    .bind(now)
    .bind(now)
    .fetch_one(db)
    .await?)
}

pub(crate) async fn find_document_by_key(
    db: &SqlitePool,
    key: &str,
) -> AppResult<Option<DocumentRow>> {
    Ok(
        sqlx::query_as::<_, DocumentRow>("SELECT * FROM documents WHERE key = ?")
            .bind(key)
            .fetch_optional(db)
            .await?,
    )
}

/// Newest first — a document list reads naturally as recent activity, not
/// an alphabetized catalog (contrast `templates::repository::list_templates`,
/// which sorts by `name` since templates are a small, named set).
///
/// Scoped to `tenant_key` (unlike `find_document_by_key`/reprint, which stay
/// opaque-key-unscoped by design) — a shared multi-tenant deployment has one
/// caller-class (`X-Internal-Api-Key`) fronting many tenants, and a list
/// endpoint is exactly the shape a missing filter would turn into a
/// cross-tenant enumeration. `tenant_key` matches `TenantKey::as_column()`'s
/// convention: `""` for single-shop/desktop, a tenant id otherwise.
pub(crate) async fn list_documents(
    db: &SqlitePool,
    tenant_key: &str,
) -> AppResult<Vec<DocumentRow>> {
    Ok(sqlx::query_as::<_, DocumentRow>(
        "SELECT * FROM documents WHERE tenant_key = ? ORDER BY created_at DESC",
    )
    .bind(tenant_key)
    .fetch_all(db)
    .await?)
}
