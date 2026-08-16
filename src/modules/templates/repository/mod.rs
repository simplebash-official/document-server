// SQLite access for the `templates` table only. Functions here never
// interpret a missing row as an error — they return `Option`/`Vec` straight
// from the driver and leave the "not found" -> `AppError` translation to
// `service`. Visibility is `pub(crate)` so `service` can call in, but the
// `mod repository;` declaration in `templates/mod.rs` is private, so none
// of this is reachable from outside the `templates` module tree.

use chrono::Utc;
use sqlx::SqlitePool;

use crate::{
    core::error::AppResult, domain::templates::TemplateType, modules::templates::model::TemplateRow,
};

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

pub(crate) async fn list_templates(
    db: &SqlitePool,
    type_filter: Option<TemplateType>,
) -> AppResult<Vec<TemplateRow>> {
    if let Some(t) = type_filter {
        Ok(sqlx::query_as::<_, TemplateRow>(
            "SELECT * FROM templates WHERE type = ? ORDER BY name ASC",
        )
        .bind(t.to_string())
        .fetch_all(db)
        .await?)
    } else {
        Ok(
            sqlx::query_as::<_, TemplateRow>("SELECT * FROM templates ORDER BY name ASC")
                .fetch_all(db)
                .await?,
        )
    }
}

/// Inserts a new `templates` row for `name` if none exists yet, or
/// updates `type`/`data_schema`/`is_active`/`updated_at` on conflict.
pub(crate) async fn upsert_template_by_name(
    db: &SqlitePool,
    name: &str,
    template_type: TemplateType,
    sample_data: Option<&serde_json::Value>,
    is_active: bool,
) -> AppResult<TemplateRow> {
    let now = Utc::now();
    let key = crate::core::id::generate_id(crate::core::constants::prefixes::TEMPLATE);
    let type_str = template_type.to_string();
    let data_schema_str = sample_data.map(|v| v.to_string());

    Ok(sqlx::query_as::<_, TemplateRow>(
        "INSERT INTO templates (key, name, type, description, data_schema, is_active, created_at, updated_at)
         VALUES (?, ?, ?, '', ?, ?, ?, ?)
         ON CONFLICT(name) DO UPDATE SET type = excluded.type, data_schema = excluded.data_schema, is_active = excluded.is_active, updated_at = excluded.updated_at
         RETURNING *",
    )
    .bind(key)
    .bind(name)
    .bind(type_str)
    .bind(data_schema_str)
    .bind(is_active)
    .bind(now)
    .bind(now)
    .fetch_one(db)
    .await?)
}
