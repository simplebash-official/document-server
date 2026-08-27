// The render pipeline itself: resolve the requested template, validate its
// payload against the template's schema contract (when it has one), compile
// it against the request's data, record the resulting document. Reaches into
// `templates`/`documents` only through their `service` (never their
// `repository`) — see spec §3.2's cross-module rule.

use std::time::Duration;

use sqlx::SqlitePool;

use crate::{
    clients::{http, render::RenderEngine},
    core::{
        config::Config,
        constants::codes,
        error::{AppError, AppResult},
    },
    modules::{
        documents,
        render::repository::{assets::StagedImage, typst, typst::TypstEngineError},
        template_data, templates,
    },
};

/// How many schema violations to include in a single rejection message
/// before truncating. A first-time integrator hitting a dozen missing fields
/// still gets the full picture without one giant blob of an error string.
const MAX_REPORTED_SCHEMA_ERRORS: usize = 10;

/// Validates `data` against the template's JSON Schema contract, returning
/// every violation in one message so an integrator fixes all fields at once
/// rather than one request at a time. Only reached when a template ships a
/// `<name>.schema.json` sidecar; schemas are compiled per call — render is
/// not hot enough to justify caching machinery, and the sidecar content can
/// change across a disk re-sync (see Phase 4's sync endpoint).
pub(crate) fn validate_against_schema(
    data_schema: &serde_json::Value,
    data: &serde_json::Value,
) -> AppResult<()> {
    let validator = jsonschema::validator_for(data_schema).map_err(|err| {
        AppError::internal_with_code(
            format!("template's own schema is invalid: {err}"),
            codes::RENDER_VALIDATION_FAILED,
        )
    })?;

    if validator.is_valid(data) {
        return Ok(());
    }

    let violations: Vec<String> = validator
        .iter_errors(data)
        .take(MAX_REPORTED_SCHEMA_ERRORS)
        .map(|err| {
            let path = err.instance_path().to_string();
            if path.is_empty() {
                err.to_string()
            } else {
                format!("at '{path}': {err}")
            }
        })
        .collect();

    Err(AppError::unprocessable_entity(
        codes::RENDER_VALIDATION_FAILED,
        format!(
            "payload does not match this template's data schema ({})",
            violations.join("; ")
        ),
    ))
}

/// Resolves `template_key` (404 if unknown/inactive), compiles it against
/// `data`, and records a `documents` entry for the successful render (see
/// spec §6.2). Returns the PDF bytes and the resolved template's `key` (the
/// caller doesn't already know it if it looked the template up by key that
/// happened to also be, e.g., `key`-normalized — kept simple: it's just
/// `template_key` echoed back today, but returning it from here rather than
/// having the route handler assume the input `template_key` is always
/// exactly what got rendered keeps this the one source of truth).
pub(crate) async fn render_template(
    db: &SqlitePool,
    render: &RenderEngine,
    cfg: &Config,
    template_key: &str,
    data: serde_json::Value,
) -> AppResult<(Vec<u8>, String)> {
    let template = templates::service::get_active_template_by_key(db, template_key).await?;

    // Stored shared/static blobs are merged under the request payload
    // (request wins) before validation and compilation — the compiled input
    // is the merged value. The `documents` record keeps the *request's*
    // payload: a reprint re-applies the merge, so updates to stored data
    // flow into reprints the same way template edits do.
    let compiled_input =
        template_data::service::merge_into_payload(db, &template.name, data.clone()).await?;

    // Validation (and any `logoUrl` fetch) lives inside `compile_pdf`, so
    // this path and the reprint path enforce the contract identically.
    let pdf_bytes = compile_pdf(
        render,
        cfg,
        &template.name,
        template.data_schema.as_ref(),
        compiled_input,
    )
    .await?;

    documents::service::record_document(db, &template.key, data, pdf_bytes.len() as i64).await?;

    Ok((pdf_bytes, template.key))
}

/// Validates `data`, resolves any remote `logoUrl` in it, compiles
/// `template_name` against the result, and translates any Typst failure into
/// the right `AppError` — shared by `render_template` above and
/// `documents::service::reprint_document` (`GET /api/documents/{key}/pdf`),
/// so both go through the exact same contract enforcement and
/// compile-error -> HTTP-status mapping instead of duplicating it.
///
/// `async` because a `logoUrl` payload triggers an outbound image download
/// (see `clients::http`); the compile itself is still synchronous CPU work.
///
/// Takes the template's `data_schema` alongside its name because validation
/// lives with compilation (`render_template` and the reprint path each
/// resolve their template themselves, and both must enforce the same
/// contract). `None` skips validation — the no-schema-sidecar behavior.
pub(crate) async fn compile_pdf(
    render: &RenderEngine,
    cfg: &Config,
    template_name: &str,
    data_schema: Option<&serde_json::Value>,
    mut data: serde_json::Value,
) -> AppResult<Vec<u8>> {
    if let Some(data_schema) = data_schema {
        validate_against_schema(data_schema, &data)?;
    }

    let logo_url = data
        .get("logoUrl")
        .and_then(|value| value.as_str())
        .filter(|url| !url.is_empty())
        .map(str::to_owned);

    if cfg.remote_image_fetch_enabled
        && let Some(url) = logo_url
    {
        // Compile through a one-shot engine so the staged image never enters
        // the shared engine's unbounded file cache.
        let rel_path = render.template_rel_path(template_name).ok_or_else(|| {
            AppError::internal(format!("no file path known for template '{template_name}'"))
        })?;
        let rel_path = rel_path.to_owned();

        let image = http::fetch_image(
            &url,
            cfg.remote_image_max_bytes,
            Duration::from_secs(cfg.remote_image_timeout_secs),
        )
        .await
        .map_err(|err| {
            AppError::unprocessable_entity(codes::REMOTE_IMAGE_FETCH_FAILED, err.to_string())
        })?;

        let staged =
            StagedImage::write(std::path::Path::new(&cfg.templates_dir), &rel_path, &image)
                .map_err(|err| {
                    AppError::internal(format!("could not stage remote image: {err}"))
                })?;

        // Point the template at the local file. `staged` (and thus the file)
        // is dropped when this block ends, whatever the compile result.
        data["logo"] = serde_json::Value::String(staged.file_name.clone());
        return typst::render_pdf_once(render, &rel_path, data).map_err(map_typst_error);
    }

    typst::render_pdf(render, template_name, data).map_err(map_typst_error)
}

fn map_typst_error(err: TypstEngineError) -> AppError {
    match err {
        TypstEngineError::Compile(msg) => {
            AppError::unprocessable_entity(codes::RENDER_VALIDATION_FAILED, msg)
        }
        TypstEngineError::Export(msg) => {
            AppError::internal_with_code(msg, codes::PDF_EXPORT_FAILED)
        }
    }
}
