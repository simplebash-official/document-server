// Business rules for the templates feature: resolving a template by key for
// rendering, and keeping the `templates` table in step with whatever
// `.typ` files are actually on disk. Delegates all SQLite access to
// `super::repository`.

use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use crate::{
    clients::render::{DiscoveredTemplate, RenderEngine},
    core::{
        config::Config,
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_template_name,
    },
    domain::templates::{
        CreateTemplateRequest, SyncTemplatesResponse, Template, TemplateType, TemplatesResponse,
    },
    modules::{render, templates::repository},
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
            tpl.description.as_deref(),
            tpl.data_schema.as_ref(),
            tpl.sample_data.as_ref(),
            true,
        )
        .await?;
    }
    Ok(())
}

/// Backs `POST /api/templates`: writes a new template (and its optional
/// sidecars) to disk under a freshly minted `<type>_temp_<nanoid>` identity,
/// then re-runs the disk sync so the row is upserted and the engine rebuilt.
/// Returns the fresh engine (the route swaps it in) and the created row.
///
/// Anything that goes wrong after the files are written rolls them back —
/// a failed create must not leave a half-published template on disk or a
/// dangling row.
pub(crate) async fn create_template(
    db: &SqlitePool,
    cfg: &Config,
    req: CreateTemplateRequest,
) -> AppResult<(RenderEngine, Template)> {
    if req.source.trim().is_empty() {
        return Err(AppError::unprocessable_entity(
            codes::TEMPLATE_CREATE_FAILED,
            "template source must not be empty",
        ));
    }

    if let Some(schema) = &req.schema
        && let Err(err) = jsonschema::validator_for(schema)
    {
        return Err(AppError::unprocessable_entity(
            codes::TEMPLATE_CREATE_FAILED,
            format!("schema is not a valid JSON Schema: {err}"),
        ));
    }

    let (subdir, prefix) = match req.r#type {
        TemplateType::Label => ("labels", prefixes::TEMPLATE_NAME_LABEL),
        TemplateType::Document => ("documents", prefixes::TEMPLATE_NAME_DOCUMENT),
    };
    let name = generate_template_name(prefix);
    let dir = Path::new(&cfg.templates_dir).join(subdir);

    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|err| AppError::internal(format!("could not create {subdir}/: {err}")))?;
    write_template_files(&dir, &name, &req)
        .await
        .map_err(|err| AppError::internal(format!("could not write template files: {err}")))?;

    // From here on, any failure must delete what we just wrote.
    let built = async {
        let (engine, _) = resync_from_disk(db, &cfg.templates_dir, &cfg.fonts_dir).await?;

        if !engine.known_templates().iter().any(|t| t.name == name) {
            return Err(AppError::unprocessable_entity(
                codes::TEMPLATE_CREATE_FAILED,
                "template was written but the render engine did not pick it up",
            ));
        }

        // Smoke-compile against the sample when one was supplied. Remote
        // image fetching is forced off here — a create should not depend on
        // an external URL being reachable.
        if let Some(sample) = req.sample.clone() {
            let smoke_cfg = Config {
                remote_image_fetch_enabled: false,
                ..cfg.clone()
            };
            render::service::compile_pdf(&engine, &smoke_cfg, &name, req.schema.as_ref(), sample)
                .await
                .map_err(|err| {
                    AppError::unprocessable_entity(
                        codes::TEMPLATE_CREATE_FAILED,
                        format!("template does not compile against its sample: {err}"),
                    )
                })?;
        }

        let row = repository::find_template_by_name(db, &name)
            .await?
            .ok_or_else(|| AppError::internal("created template row vanished immediately"))?;
        Ok((engine, row.into_template()))
    }
    .await;

    match built {
        Ok(result) => Ok(result),
        Err(err) => {
            remove_template_files(&dir, &name).await;
            // Best-effort: bring the engine/DB back to the pre-create state.
            let _ = resync_from_disk(db, &cfg.templates_dir, &cfg.fonts_dir).await;
            Err(err)
        }
    }
}

async fn write_template_files(
    dir: &Path,
    name: &str,
    req: &CreateTemplateRequest,
) -> std::io::Result<()> {
    tokio::fs::write(dir.join(format!("{name}.typ")), &req.source).await?;
    if let Some(schema) = &req.schema {
        let pretty = serde_json::to_vec_pretty(schema).unwrap_or_default();
        tokio::fs::write(dir.join(format!("{name}.schema.json")), pretty).await?;
    }
    if let Some(sample) = &req.sample {
        let pretty = serde_json::to_vec_pretty(sample).unwrap_or_default();
        tokio::fs::write(dir.join(format!("{name}.json")), pretty).await?;
    }
    Ok(())
}

async fn remove_template_files(dir: &Path, name: &str) {
    for ext in ["typ", "schema.json", "json"] {
        let path: PathBuf = dir.join(format!("{name}.{ext}"));
        if let Err(err) = tokio::fs::remove_file(&path).await
            && err.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(path = %path.display(), %err, "failed to roll back template file");
        }
    }
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
