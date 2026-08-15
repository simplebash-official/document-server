// Business rules for the documents feature. Currently just the one write
// path: recording a successful render. Delegates all SQLite access to
// `super::repository`.

use sqlx::SqlitePool;

use crate::{
    core::{constants::prefixes, error::AppResult, id::generate_id},
    domain::documents::Document,
    modules::documents::repository,
};

/// Records a successful render. Stores `data`, not the PDF bytes — a
/// reprint always reflects the *current* template (a fixed typo in the
/// template benefits every historical document), and this avoids needing
/// blob storage for a first version. Called by
/// `modules::render::service::render_template` after a render succeeds; see
/// spec §6.2.
pub(crate) async fn record_document(
    db: &SqlitePool,
    template_key: &str,
    data: serde_json::Value,
    file_size_bytes: i64,
) -> AppResult<Document> {
    let key = generate_id(prefixes::DOCUMENT);
    let row = repository::insert_document(db, &key, template_key, data, file_size_bytes).await?;
    Ok(row.into_document())
}
