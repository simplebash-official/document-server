// Business rules for the templates feature: resolving a template by key for
// rendering, and keeping the `templates` table in step with whatever
// `.typ` files are actually on disk. Delegates all SQLite access to
// `super::repository`.

use sqlx::SqlitePool;

use crate::{
    clients::render::DiscoveredTemplate,
    core::{
        constants::codes,
        error::{AppError, AppResult},
    },
    domain::templates::{Template, TemplateType, TemplatesResponse},
    modules::templates::repository,
};

/// Every known template, optionally filtered by `type` — backs `GET /api/templates`.
/// Returns inactive templates too; `render`'s `get_active_template_by_key` is the
/// one place that filters on `is_active`.
pub(crate) async fn list_templates(
    db: &SqlitePool,
    type_filter: Option<TemplateType>,
) -> AppResult<TemplatesResponse> {
    let rows = repository::list_templates(db, type_filter).await?;
    Ok(TemplatesResponse {
        templates: rows.into_iter().map(|row| row.into_template()).collect(),
    })
}

/// Fetch by `key` for `GET /api/templates/{key}` — 404s with
/// `TEMPLATE_NOT_FOUND` if missing, but does not filter on `is_active`.
pub(crate) async fn get_template_by_key(db: &SqlitePool, key: &str) -> AppResult<Template> {
    let row = repository::find_template_by_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Template not found", codes::TEMPLATE_NOT_FOUND)
        })?;

    Ok(row.into_template())
}

/// Fetch by `key`, 404ing with `TEMPLATE_NOT_FOUND` if missing *or*
/// inactive.
pub(crate) async fn get_active_template_by_key(db: &SqlitePool, key: &str) -> AppResult<Template> {
    let row = repository::find_template_by_key(db, key)
        .await?
        .filter(|row| row.is_active)
        .ok_or_else(|| {
            AppError::not_found_with_code("Template not found", codes::TEMPLATE_NOT_FOUND)
        })?;

    Ok(row.into_template())
}

/// Upserts one `templates` row per discovered template in `known_templates`,
/// marking each active.
pub async fn sync_templates_from_disk(
    db: &SqlitePool,
    known_templates: &[DiscoveredTemplate],
) -> AppResult<()> {
    for tpl in known_templates {
        repository::upsert_template_by_name(
            db,
            &tpl.name,
            tpl.template_type,
            tpl.sample_data.as_ref(),
            true,
        )
        .await?;
    }
    Ok(())
}
