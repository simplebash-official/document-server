// SQLite row shape for the `templates` table. Kept separate from
// `domain::templates` (the API-facing type) purely for symmetry with every
// other module's `model.rs` — unlike a Mongo document, a row here has no
// framework-specific type (like `ObjectId`) that needs to stay out of API
// payloads, since `key` (not a separate autoincrement id) is the table's
// primary key.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use sqlx::types::Json;

use crate::domain::templates::Template;

#[derive(Debug, Clone, FromRow)]
pub struct TemplateRow {
    pub key: String,
    /// Matches the `.typ` filename stem on disk (e.g. `"receipt"` for
    /// `templates/receipt.typ`) — how `templates::service` resolves which
    /// source file to compile.
    pub name: String,
    pub description: String,
    pub data_schema: Option<Json<serde_json::Value>>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TemplateRow {
    pub fn into_template(self) -> Template {
        Template {
            key: self.key,
            name: self.name,
            description: self.description,
            data_schema: self.data_schema.map(|Json(value)| value),
            is_active: self.is_active,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
