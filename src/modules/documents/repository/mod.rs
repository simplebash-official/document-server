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
    template_key: &str,
    data: serde_json::Value,
    file_size_bytes: i64,
) -> AppResult<DocumentRow> {
    let now = Utc::now();

    Ok(sqlx::query_as::<_, DocumentRow>(
        "INSERT INTO documents (key, template_key, data, file_size_bytes, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)
         RETURNING *",
    )
    .bind(key)
    .bind(template_key)
    .bind(Json(data))
    .bind(file_size_bytes)
    .bind(now)
    .bind(now)
    .fetch_one(db)
    .await?)
}

#[allow(dead_code)] // scaffolded for Phase 3's real `GET /api/documents/{key}` read route.
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
