// Business rules for the templates feature: which templates a tenant may
// see, render or reprint, keeping the bundled (global) templates in step with
// the shipped files, and the write operations the packages module builds on.
// Delegates all SQLite access to `super::repository`.

use sqlx::SqlitePool;

use crate::{
    clients::render::BundledTemplate,
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::templates::{Template, TemplateScope, TemplateType, TemplatesResponse},
    modules::templates::{model::TemplateRow, repository},
};

fn not_found() -> AppError {
    AppError::not_found_with_code("Template not found", codes::TEMPLATE_NOT_FOUND)
}

pub(crate) fn into_template(row: TemplateRow) -> AppResult<Template> {
    row.try_into_template()
        .map_err(|err| AppError::internal(format!("template row is corrupt: {err}")))
}

/// The latest published version of every family `tenant_key` may render,
/// optionally filtered by type — backs `GET /api/templates`.
pub(crate) async fn list_templates(
    db: &SqlitePool,
    tenant_key: &str,
    type_filter: Option<TemplateType>,
) -> AppResult<TemplatesResponse> {
    let type_filter = type_filter.map(|t| t.to_string());
    let rows = repository::list_renderable_latest(db, tenant_key, type_filter.as_deref()).await?;
    let templates = rows
        .into_iter()
        .map(into_template)
        .collect::<AppResult<Vec<_>>>()?;
    Ok(TemplatesResponse { templates })
}

/// Backs `GET /api/templates/{key}` — 404 unless the tenant may render it.
pub(crate) async fn get_template_by_key(
    db: &SqlitePool,
    tenant_key: &str,
    key: &str,
) -> AppResult<Template> {
    into_template(resolve_renderable(db, tenant_key, key).await?)
}

/// The published template `key` if `tenant_key` may render it, else 404.
/// A template that exists but is not visible to the tenant is
/// indistinguishable from one that does not exist.
pub(crate) async fn resolve_renderable(
    db: &SqlitePool,
    tenant_key: &str,
    key: &str,
) -> AppResult<TemplateRow> {
    repository::find_renderable_by_key(db, tenant_key, key)
        .await?
        .ok_or_else(not_found)
}

/// The template `key` for reprinting a document of `tenant_key` — status is
/// ignored (a retired design still reprints), visibility is not.
pub(crate) async fn resolve_reprintable(
    db: &SqlitePool,
    tenant_key: &str,
    key: &str,
) -> AppResult<TemplateRow> {
    repository::find_reprintable_by_key(db, tenant_key, key)
        .await?
        .ok_or_else(not_found)
}

/// The template called `name` if `tenant_key` may use it (any status), else
/// 404 — scopes tenant data that is keyed by template name.
pub(crate) async fn ensure_usable_by(
    db: &SqlitePool,
    tenant_key: &str,
    name: &str,
) -> AppResult<TemplateRow> {
    let row = repository::find_by_name(db, name)
        .await?
        .ok_or_else(not_found)?;
    let scope = row
        .scope()
        .map_err(|err| AppError::internal(format!("template row is corrupt: {err}")))?;
    if scope == TemplateScope::Private
        && !repository::is_granted(db, &row.family, tenant_key).await?
    {
        return Err(not_found());
    }
    Ok(row)
}

/// The template called `name`, no visibility filter — the admin package
/// routes, which are internal-caller-only.
pub(crate) async fn get_by_name(db: &SqlitePool, name: &str) -> AppResult<TemplateRow> {
    repository::find_by_name(db, name)
        .await?
        .ok_or_else(not_found)
}

pub(crate) async fn list_all(db: &SqlitePool) -> AppResult<Vec<TemplateRow>> {
    repository::list_all(db).await
}

pub(crate) async fn list_grants(db: &SqlitePool) -> AppResult<Vec<(String, String)>> {
    repository::list_grants(db).await
}

/// Publishes the bundled templates found on disk at boot: each is upserted as
/// a global, published template, and any global template that is no longer
/// shipped is retired (history stays reprintable).
pub async fn sync_bundled(db: &SqlitePool, bundled: &[BundledTemplate]) -> AppResult<()> {
    let now = chrono::Utc::now();
    for tpl in bundled {
        let row = TemplateRow {
            key: generate_id(prefixes::TEMPLATE),
            name: tpl.name.clone(),
            family: tpl.name.clone(),
            version: 1,
            r#type: tpl.template_type.to_string(),
            kind: tpl.kind.clone(),
            description: tpl.title.clone(),
            data_schema: sqlx::types::Json(tpl.data_schema.clone()),
            sample_data: sqlx::types::Json(tpl.sample_data.clone()),
            status: "published".to_string(),
            scope: "global".to_string(),
            file_path: tpl.file_path.clone(),
            sha256: tpl.sha256.clone(),
            min_app_version: None,
            created_at: now,
            updated_at: now,
        };
        repository::upsert_global(db, &row).await?;
    }
    let names: Vec<String> = bundled.iter().map(|t| t.name.clone()).collect();
    repository::retire_globals_not_in(db, &names).await?;
    Ok(())
}

pub(crate) async fn next_version(db: &SqlitePool, family: &str) -> AppResult<i64> {
    Ok(repository::max_version(db, family).await?.unwrap_or(0) + 1)
}

pub(crate) async fn title_used_by_other_family(
    db: &SqlitePool,
    title: &str,
    family: &str,
) -> AppResult<bool> {
    repository::title_used_by_other_family(db, title, family).await
}

/// Inserts a private package version. A concurrent publish of the same
/// `(family, version)` loses on the unique index and is reported as a
/// conflict instead of an internal error.
pub(crate) async fn insert_private(db: &SqlitePool, row: &TemplateRow) -> AppResult<()> {
    let mut tx = db.begin().await?;
    repository::insert_private(&mut tx, row)
        .await
        .map_err(|err| match &err {
            sqlx::Error::Database(db_err) if db_err.is_unique_violation() => AppError::custom(
                axum::http::StatusCode::CONFLICT,
                codes::PACKAGE_CONFLICT,
                "A package with this name or version already exists",
            ),
            _ => err.into(),
        })?;
    tx.commit().await?;
    Ok(())
}

pub(crate) async fn retire_family(db: &SqlitePool, family: &str) -> AppResult<u64> {
    repository::retire_family(db, family).await
}

pub(crate) async fn replace_grants(
    db: &SqlitePool,
    family: &str,
    tenant_keys: &[String],
) -> AppResult<()> {
    repository::replace_grants(db, family, tenant_keys).await
}

pub(crate) async fn add_grant(db: &SqlitePool, family: &str, tenant_key: &str) -> AppResult<()> {
    repository::add_grant(db, family, tenant_key).await
}

/// The highest-version row of `family`, if the family exists.
pub(crate) async fn family_head(db: &SqlitePool, family: &str) -> AppResult<Option<TemplateRow>> {
    repository::find_family_head(db, family).await
}
