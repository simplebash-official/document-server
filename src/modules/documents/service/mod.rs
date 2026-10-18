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
    tenant_key: &str,
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
    let row =
        repository::insert_document(db, &key, tenant_key, template_key, data, file_size_bytes)
            .await?;
    Ok(row.into_document())
}

/// Every recorded document belonging to `tenant_key`, newest first — backs
/// `GET /api/documents`. See `repository::list_documents` for why this is
/// the one documents-read that IS tenant-scoped.
pub(crate) async fn list_documents(
    db: &SqlitePool,
    tenant_key: &str,
) -> AppResult<DocumentsResponse> {
    let rows = repository::list_documents(db, tenant_key).await?;
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
/// recompiling against the template it was first rendered with, not a cached
/// copy of the original PDF bytes (none are stored — see spec §6.2). Backs
/// `GET /api/documents/{key}/pdf`.
///
/// A published template is immutable (a design change is a new template row),
/// so the reprint uses exactly the design the document was rendered with.
///
/// Resolved through `templates::service::resolve_reprintable` — deliberately
/// *not* the renderable variant `render::service::render_template` uses:
/// retiring a template is an editorial decision about future renders, not a
/// statement that history rendered against it stops being reprintable, so a
/// retired template must still work here. Tenant visibility still applies,
/// against the DOCUMENT's own recorded tenant.
pub(crate) async fn reprint_document(
    db: &SqlitePool,
    render_engine: &RenderEngine,
    cfg: &Config,
    key: &str,
) -> AppResult<Vec<u8>> {
    let document = get_document_by_key(db, key).await?;
    let template =
        templates::service::resolve_reprintable(db, &document.tenant_key, &document.template_key)
            .await?;

    // The stored `data` is the original *request* payload (see
    // `render_template`), so the stored-data merge — and any `logoUrl`
    // re-fetch inside `compile_pdf` — re-applies here exactly as it did on
    // the first render. Updates to stored blobs (or to the remote image at
    // that URL) flow into reprints without touching history. Merged against
    // the DOCUMENT's own recorded `tenant_key`, not whatever `X-Tenant-Key`
    // this reprint call happens to carry — a document's key is opaque and
    // unscoped on lookup (see `DocumentRow::tenant_key`'s doc comment), so
    // trusting the caller's header here would let a forged/mismatched header
    // pull a *different* tenant's stored blobs into this reprint.
    let compiled_input = template_data::service::merge_into_payload(
        db,
        &document.tenant_key,
        &template.name,
        document.data,
    )
    .await?;

    render::service::compile_pdf(render_engine, cfg, &template, compiled_input).await
}
