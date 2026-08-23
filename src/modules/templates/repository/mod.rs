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

/// Lookup by *name* (the `.typ` stem) — the identity callers resolve when
/// rendering and when scoping stored template data.
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
/// updates `type`/`data_schema`/`sample_data`/`is_active`/`updated_at` on
/// conflict.
pub(crate) async fn upsert_template_by_name(
    db: &SqlitePool,
    name: &str,
    template_type: TemplateType,
    data_schema: Option<&serde_json::Value>,
    sample_data: Option<&serde_json::Value>,
    is_active: bool,
) -> AppResult<TemplateRow> {
    let now = Utc::now();
    let key = crate::core::id::generate_id(crate::core::constants::prefixes::TEMPLATE);
    let type_str = template_type.to_string();
    let schema_str = data_schema.map(|v| v.to_string());
    let sample_str = sample_data.map(|v| v.to_string());

    Ok(sqlx::query_as::<_, TemplateRow>(
        "INSERT INTO templates (key, name, type, description, data_schema, sample_data, is_active, created_at, updated_at)
         VALUES (?, ?, ?, '', ?, ?, ?, ?, ?)
         ON CONFLICT(name) DO UPDATE SET type = excluded.type, data_schema = excluded.data_schema, sample_data = excluded.sample_data, is_active = excluded.is_active, updated_at = excluded.updated_at
         RETURNING *",
    )
    .bind(key)
    .bind(name)
    .bind(type_str)
    .bind(schema_str)
    .bind(sample_str)
    .bind(is_active)
    .bind(now)
    .bind(now)
    .fetch_one(db)
    .await?)
}

/// Marks every currently-active template whose name is NOT in `names` as
/// inactive — the disk-sync counterpart for `.typ` files that were removed.
/// Rows are never deleted: a template's `key` is stable and historical
/// `documents` rows reference it, so removal from disk means "no future
/// renders", not "history erased".
pub(crate) async fn deactivate_templates_not_in(
    db: &SqlitePool,
    names: &[String],
) -> AppResult<u64> {
    let now = Utc::now();

    // `NOT IN ()` isn't valid SQL — an empty disk scan deactivates everything.
    if names.is_empty() {
        let result =
            sqlx::query("UPDATE templates SET is_active = 0, updated_at = ? WHERE is_active = 1")
                .bind(now)
                .execute(db)
                .await?;
        return Ok(result.rows_affected());
    }

    let placeholders = vec!["?"; names.len()].join(", ");
    let sql = format!(
        "UPDATE templates SET is_active = 0, updated_at = ? WHERE is_active = 1 AND name NOT IN ({placeholders})"
    );
    // `AssertSqlSafe` is justified: the only interpolated content is the
    // placeholder list itself — every value travels as a bind parameter.
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(now);
    for name in names {
        query = query.bind(name);
    }

    Ok(query.execute(db).await?.rows_affected())
}
