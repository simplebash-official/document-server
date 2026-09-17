// Business rules for the documents feature: recording a successful render,
// and (as of Phase 3) reading records back — list, get one, and reprint.
// Delegates all SQLite access to `super::repository`.

use sqlx::SqlitePool;

use crate::{
    clients::render::RenderEngine,
    core::{
        config::Config,
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::documents::{Document, DocumentsResponse},
    modules::{documents::repository, render, template_data, templates},
};

/// Records a successful render. Stores `data`, not the PDF bytes — a
/// reprint always reflects the *current* template (a fixed typo in the
/// template benefits every historical document), and this avoids needing
/// blob storage for a first version. Called by
/// `modules::render::service::render_template` after a render succeeds; see
/// spec §6.2.
///
/// One field is *not* stored verbatim: a `logoUrl` that is a `data:` URI
/// (an inline base64 image — SimpleBash POS sends its shop logo this way) is
/// blanked before persisting. Such a blob is tens–hundreds of KB and would
/// land on every recorded row; the caller is the source of truth for the
/// logo and simply re-sends it on the next render. An http(s) `logoUrl` is
/// kept as-is — it's cheap and a reprint needs it.
pub(crate) async fn record_document(
    db: &SqlitePool,
    template_key: &str,
    mut data: serde_json::Value,
    file_size_bytes: i64,
) -> AppResult<Document> {
    if let Some(logo_url) = data.get("logoUrl").and_then(|v| v.as_str())
        && logo_url.starts_with("data:")
    {
        data["logoUrl"] = serde_json::Value::String(String::new());
    }

    let key = generate_id(prefixes::DOCUMENT);
    let row = repository::insert_document(db, &key, template_key, data, file_size_bytes).await?;
    Ok(row.into_document())
}

/// Every recorded document, newest first — backs `GET /api/documents`.
pub(crate) async fn list_documents(db: &SqlitePool) -> AppResult<DocumentsResponse> {
    let rows = repository::list_documents(db).await?;
    Ok(DocumentsResponse {
        documents: rows.into_iter().map(|row| row.into_document()).collect(),
    })
}

/// Fetch by `key` for `GET /api/documents/{key}` — 404s with
/// `DOCUMENT_NOT_FOUND` if missing.
pub(crate) async fn get_document_by_key(db: &SqlitePool, key: &str) -> AppResult<Document> {
    let row = repository::find_document_by_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Document not found", codes::DOCUMENT_NOT_FOUND)
        })?;

    Ok(row.into_document())
}

/// Re-renders a previously-recorded document from its stored `data` by
/// recompiling against the template, not a cached copy of the original PDF
/// bytes (none are stored — see spec §6.2). Backs `GET /api/documents/{key}/pdf`.
///
/// "Recompiling" here means "against whatever `RenderEngine` loaded at its
/// last `warm_up()`", not literally the `.typ` file's current on-disk bytes
/// — `with_file_system_resolver`'s underlying resolver is
/// `.into_cached()`'d by `typst-as-lib`, so a template edited on disk while
/// the server keeps running is *not* picked up until a restart. A template
/// fix followed by a restart benefits every historical document's reprint,
/// same idea as SimpleBash POS's usual "genuinely fresh, not stale" guarantees,
/// just scoped to "fresh as of last boot" rather than "fresh this instant"
/// — true hot-reload is unimplemented Phase 4 work (see CLAUDE.md's Build
/// Phases).
///
/// Looked up via `templates::service::get_template_by_key` — deliberately
/// *not* the `_active_` variant `render::service::render_template` uses:
/// deactivating a template is an editorial decision about future renders,
/// not a statement that history rendered against it stops being
/// reprintable, so an inactive template must still work here.
pub(crate) async fn reprint_document(
    db: &SqlitePool,
    render_engine: &RenderEngine,
    cfg: &Config,
    key: &str,
) -> AppResult<Vec<u8>> {
    let document = get_document_by_key(db, key).await?;
    let template = templates::service::get_template_by_key(db, &document.template_key).await?;

    // The stored `data` is the original *request* payload (see
    // `render_template`), so the stored-data merge — and any `logoUrl`
    // re-fetch inside `compile_pdf` — re-applies here exactly as it did on
    // the first render. Updates to stored blobs (or to the remote image at
    // that URL) flow into reprints without touching history.
    let compiled_input =
        template_data::service::merge_into_payload(db, &template.name, document.data).await?;

    render::service::compile_pdf(
        render_engine,
        cfg,
        &template.name,
        template.data_schema.as_ref(),
        compiled_input,
    )
    .await
}
