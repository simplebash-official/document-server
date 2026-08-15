// SQLite row shape for the `documents` table. Kept separate from
// `domain::documents` (the API-facing type) purely for symmetry with every
// other module's `model.rs`.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use sqlx::types::Json;

use crate::domain::documents::Document;

#[derive(Debug, Clone, FromRow)]
pub struct DocumentRow {
    pub key: String,
    /// References `templates.key` — foreign key by `key`, never by name or
    /// row id (see spec §5.3).
    pub template_key: String,
    /// The exact request payload this document was rendered with — stored
    /// for reprint/audit instead of the PDF bytes (see spec §6.2).
    pub data: Json<serde_json::Value>,
    pub file_size_bytes: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl DocumentRow {
    pub fn into_document(self) -> Document {
        Document {
            key: self.key,
            template_key: self.template_key,
            data: self.data.0,
            file_size_bytes: self.file_size_bytes,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
