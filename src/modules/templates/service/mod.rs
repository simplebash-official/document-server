// Business rules for the templates feature: resolving a template by key for
// rendering, and keeping the `templates` table in step with whatever
// `.typ` files are actually on disk. Delegates all SQLite access to
// `super::repository`.

use sqlx::SqlitePool;

use crate::{
    core::{
        constants::codes,
        error::{AppError, AppResult},
    },
    domain::templates::Template,
    modules::templates::repository,
};

/// Fetch by `key`, 404ing with `TEMPLATE_NOT_FOUND` if missing *or*
/// inactive — an inactive template is treated the same as a nonexistent one
/// from a caller's perspective. This is the entry point
/// `modules::render::service` calls before compiling.
pub(crate) async fn get_active_template_by_key(db: &SqlitePool, key: &str) -> AppResult<Template> {
    let row = repository::find_template_by_key(db, key)
        .await?
        .filter(|row| row.is_active)
        .ok_or_else(|| {
            AppError::not_found_with_code("Template not found", codes::TEMPLATE_NOT_FOUND)
        })?;

    Ok(row.into_template())
}

/// Upserts one `templates` row per name in `known_templates` (the `.typ`
/// filename stems `clients::render::RenderEngine::warm_up` found on disk),
/// marking each active. Called once from `main.rs` after the render engine
/// warms up, and again by `tests::common::spawn_app` for the test
/// database — `pub`, not `pub(crate)`, because `main.rs` is a separate
/// binary crate under the lib/bin split and can't reach a `pub(crate)` item
/// in the `pdf_server` library crate.
///
/// Deliberately does not deactivate a `templates` row whose `.typ` file was
/// removed from disk — that's an operator decision (delete or deliberately
/// deactivate), not something a routine sync should do silently, since a
/// document referencing that template's key may still need to be looked up
/// for audit purposes.
pub async fn sync_templates_from_disk(
    db: &SqlitePool,
    known_templates: &[String],
) -> AppResult<()> {
    for name in known_templates {
        repository::upsert_template_by_name(db, name, true).await?;
    }
    Ok(())
}
