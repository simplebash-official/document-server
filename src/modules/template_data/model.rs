// SQLite row shape for the `template_data` table. Kept separate from
// `domain::template_data` (the API-facing type) for symmetry with every
// other module's `model.rs`.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use sqlx::types::Json;

use crate::domain::template_data::TemplateData;

#[derive(Debug, Clone, FromRow)]
pub struct TemplateDataRow {
    pub key: String,
    /// The owning template's *name* (its stable disk identity), not its
    /// `tpl_...` key — callers resolve templates by name when rendering, and
    /// a template's row can be re-synced without changing identity.
    pub template_name: String,
    /// Caller-chosen name of this blob within the template's namespace.
    pub data_key: String,
    pub data: Json<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TemplateDataRow {
    pub fn into_template_data(self) -> TemplateData {
        TemplateData {
            key: self.key,
            template_name: self.template_name,
            data_key: self.data_key,
            data: self.data.0,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
