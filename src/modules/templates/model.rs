// SQLite row shape for the `templates` table. Kept separate from
// `domain::templates` (the API-facing type) purely for symmetry with every
// other module's `model.rs` — unlike a Mongo document, a row here has no
// framework-specific type (like `ObjectId`) that needs to stay out of API
// payloads, since `key` (not a separate autoincrement id) is the table's
// primary key.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use sqlx::types::Json;

use crate::domain::templates::{Template, TemplateType};

#[derive(Debug, Clone, FromRow)]
pub struct TemplateRow {
    pub key: String,
    /// Matches the `.typ` filename or relative stem on disk (e.g. `"receipt"` or `"sticker"`)
    pub name: String,
    /// Template categorization (`"document"` or `"label"`)
    pub r#type: String,
    pub description: String,
    /// The template's machine-readable input contract, synced from the
    /// `<name>.schema.json` sidecar. Historically this column held sample
    /// data under a misleading name; the disk sync rewrites it with the real
    /// schema on first boot after that change (see `clients::sqlite`).
    pub data_schema: Option<Json<serde_json::Value>>,
    /// Worked example of valid input, synced from the `<name>.json` sidecar.
    pub sample_data: Option<Json<serde_json::Value>>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TemplateRow {
    pub fn into_template(self) -> Template {
        let template_type = self
            .r#type
            .parse::<TemplateType>()
            .unwrap_or(TemplateType::Document);

        Template {
            key: self.key,
            name: self.name,
            r#type: template_type,
            description: self.description,
            data_schema: self.data_schema.map(|Json(value)| value),
            data: self.sample_data.map(|Json(value)| value),
            is_active: self.is_active,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
