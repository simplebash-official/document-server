// The render pipeline itself: resolve the requested template, validate its
// payload against the template's schema contract (when it has one), compile
// it against the request's data, record the resulting document. Reaches into
// `templates`/`documents` only through their `service` (never their
// `repository`) — see spec §3.2's cross-module rule.

use sqlx::SqlitePool;

use crate::{
    clients::render::RenderEngine,
    core::{
        constants::codes,
        error::{AppError, AppResult},
    },
    modules::{
        documents, render::repository::typst, render::repository::typst::TypstEngineError,
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

    // Validation lives inside `compile_pdf`, so this path and the reprint
    // path enforce the contract identically.
    let pdf_bytes = compile_pdf(
        render,
        &template.name,
        template.data_schema.as_ref(),
        compiled_input,
    )?;

    documents::service::record_document(db, &template.key, data, pdf_bytes.len() as i64).await?;

    Ok((pdf_bytes, template.key))
}

/// Compiles `template_name` against `data` and translates any Typst
/// failure into the right `AppError` — shared by `render_template` above
/// and `documents::service::reprint_document` (Phase 3's
/// `GET /api/documents/{key}/pdf`), so both go through the exact same
/// compile-error -> HTTP-status mapping instead of duplicating it. Not
/// `async`: `repository::typst::render_pdf` has no `.await` points of its
/// own — Typst compilation is synchronous, CPU-bound work (see spec §9).
///
/// Takes the template's `data_schema` alongside its name because validation
/// lives with compilation (`render_template` and the reprint path each
/// resolve their template themselves, and both must enforce the same
/// contract). `None` skips validation — the no-schema-sidecar behavior.
pub(crate) fn compile_pdf(
    render: &RenderEngine,
    template_name: &str,
    data_schema: Option<&serde_json::Value>,
    data: serde_json::Value,
) -> AppResult<Vec<u8>> {
    if let Some(data_schema) = data_schema {
        validate_against_schema(data_schema, &data)?;
    }

    typst::render_pdf(render, template_name, data).map_err(|err| match err {
        TypstEngineError::Compile(msg) => {
            AppError::unprocessable_entity(codes::RENDER_VALIDATION_FAILED, msg)
        }
        TypstEngineError::Export(msg) => {
            AppError::internal_with_code(msg, codes::PDF_EXPORT_FAILED)
        }
    })
}
