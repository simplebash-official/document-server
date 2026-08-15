// SQLite access for the `templates` table only. Functions here never
// interpret a missing row as an error — they return `Option`/`Vec` straight
// from the driver and leave the "not found" -> `AppError` translation to
// `service`. Visibility is `pub(crate)` so `service` can call in, but the
// `mod repository;` declaration in `templates/mod.rs` is private, so none
// of this is reachable from outside the `templates` module tree.

use chrono::Utc;
use sqlx::SqlitePool;

use crate::{core::error::AppResult, modules::templates::model::TemplateRow};

pub(crate) async fn find_template_by_key(
    db: &SqlitePool,
    key: &str,
) -> AppResult<Option<TemplateRow>> {
    Ok(
        sqlx::query_as::<_, TemplateRow>("SELECT * FROM templates WHERE key = ?")
            .bind(key)
            .fetch_optional(db)
            .await?,
    )
}

#[allow(dead_code)] // scaffolded for Phase 2's real `GET /api/templates` read routes.
pub(crate) async fn find_template_by_name(
    db: &SqlitePool,
    name: &str,
) -> AppResult<Option<TemplateRow>> {
    Ok(
        sqlx::query_as::<_, TemplateRow>("SELECT * FROM templates WHERE name = ?")
            .bind(name)
            .fetch_optional(db)
            .await?,
    )
}

#[allow(dead_code)] // scaffolded for Phase 2's real `GET /api/templates` read routes.
pub(crate) async fn list_templates(db: &SqlitePool) -> AppResult<Vec<TemplateRow>> {
    Ok(
        sqlx::query_as::<_, TemplateRow>("SELECT * FROM templates ORDER BY name ASC")
            .fetch_all(db)
            .await?,
    )
}

/// Inserts a new `templates` row for `name` if none exists yet, or
/// otherwise only touches `is_active`/`updated_at` — never clobbers a
/// hand-edited `description`/`data_schema` just because the on-disk sync
/// (`service::sync_templates_from_disk`) ran again. `ON CONFLICT ...
/// RETURNING *` does the "insert or update, then read back the result" in
/// one round trip.
pub(crate) async fn upsert_template_by_name(
    db: &SqlitePool,
    name: &str,
    is_active: bool,
) -> AppResult<TemplateRow> {
    let now = Utc::now();
    let key = crate::core::id::generate_id(crate::core::constants::prefixes::TEMPLATE);

    Ok(sqlx::query_as::<_, TemplateRow>(
        "INSERT INTO templates (key, name, description, data_schema, is_active, created_at, updated_at)
         VALUES (?, ?, '', NULL, ?, ?, ?)
         ON CONFLICT(name) DO UPDATE SET is_active = excluded.is_active, updated_at = excluded.updated_at
         RETURNING *",
    )
    .bind(key)
    .bind(name)
    .bind(is_active)
    .bind(now)
    .bind(now)
    .fetch_one(db)
    .await?)
}
