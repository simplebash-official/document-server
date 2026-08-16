// The render pipeline itself: resolve the requested template, compile it
// against the request's data, record the resulting document. Reaches into
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
        templates,
    },
};

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

    let pdf_bytes = compile_pdf(render, &template.name, data.clone())?;

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
pub(crate) fn compile_pdf(
    render: &RenderEngine,
    template_name: &str,
    data: serde_json::Value,
) -> AppResult<Vec<u8>> {
    typst::render_pdf(render, template_name, data).map_err(|err| match err {
        TypstEngineError::Compile(msg) => {
            AppError::unprocessable_entity(codes::RENDER_VALIDATION_FAILED, msg)
        }
        TypstEngineError::Export(msg) => {
            AppError::internal_with_code(msg, codes::PDF_EXPORT_FAILED)
        }
    })
}
