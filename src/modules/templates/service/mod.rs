// Business rules for the templates feature: resolving a template by key for
// rendering, and keeping the `templates` table in step with whatever
// `.typ` files are actually on disk. Delegates all SQLite access to
// `super::repository`.

use sqlx::SqlitePool;

use crate::{
    clients::render::{DiscoveredTemplate, RenderEngine},
    core::{
        constants::codes,
        error::{AppError, AppResult},
    },
    domain::templates::{SyncTemplatesResponse, Template, TemplateType, TemplatesResponse},
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

/// Fetch by *name* for callers that resolve templates the way rendering
/// does (`.typ` stem, e.g. stored template data scoping). Not active-filtered.
pub(crate) async fn get_template_by_name(db: &SqlitePool, name: &str) -> AppResult<Template> {
    let row = repository::find_template_by_name(db, name)
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
/// marking each active. Each row carries both the machine-readable input
/// contract (`data_schema`, from the `<name>.schema.json` sidecar) and the
/// sample data (`sample_data`, from `<name>.json`) — the sync pass is what
/// migrates an old database's misnamed sample-only `data_schema` column to
/// the split layout.
pub async fn sync_templates_from_disk(
    db: &SqlitePool,
    known_templates: &[DiscoveredTemplate],
) -> AppResult<()> {
    for tpl in known_templates {
        repository::upsert_template_by_name(
            db,
            &tpl.name,
            tpl.template_type,
            tpl.data_schema.as_ref(),
            tpl.sample_data.as_ref(),
            true,
        )
        .await?;
    }
    Ok(())
}

/// Runtime disk re-sync backing `POST /api/templates/sync`: rebuilds the
/// render engine from `templates_dir` (so new and edited `.typ` files are
/// picked up without a process restart — closing the file-resolver caching
/// gap), upserts every discovered template, deactivates rows whose `.typ`
/// disappeared, and returns the fresh engine for the caller to swap in.
///
/// The engine is *returned*, not installed: swapping it into `AppState`
/// stays the route's job so a failed sync leaves the old engine serving
/// untouched. Deactivation runs only after a successful warm-up — a
/// transiently-unreadable templates directory must not deactivate anything.
pub async fn resync_from_disk(
    db: &SqlitePool,
    templates_dir: &str,
    fonts_dir: &str,
) -> AppResult<(RenderEngine, SyncTemplatesResponse)> {
    let engine = RenderEngine::warm_up(templates_dir, fonts_dir).map_err(|err| {
        AppError::internal(format!("failed to rebuild render engine from disk: {err}"))
    })?;

    let names: Vec<String> = engine
        .known_templates()
        .iter()
        .map(|tpl| tpl.name.clone())
        .collect();

    sync_templates_from_disk(db, engine.known_templates()).await?;
    let deactivated_count = repository::deactivate_templates_not_in(db, &names).await?;

    Ok((
        engine,
        SyncTemplatesResponse {
            synced_count: names.len() as u64,
            deactivated_count,
            templates: names,
        },
    ))
}
