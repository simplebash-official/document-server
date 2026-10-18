// SQLite row shape for the `templates` table. Kept separate from
// `domain::templates` (the API-facing type) purely for symmetry with every
// other module's `model.rs` — unlike a Mongo document, a row here has no
// framework-specific type (like `ObjectId`) that needs to stay out of API
// payloads, since `key` (not a separate autoincrement id) is the table's
// primary key.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use sqlx::types::Json;

use crate::domain::templates::{
    Template, TemplateScope, TemplateStatus, TemplateType, parse_options,
};

#[derive(Debug, Clone, FromRow)]
pub struct TemplateRow {
    pub key: String,
    /// Generated identity (`doc_temp_<nanoid>` / `lbl_temp_<nanoid>`); for a
    /// bundled template it matches the `.typ` stem on disk.
    pub name: String,
    /// Lineage of one design's versions; a bundled template is its own
    /// one-version family (`family == name`).
    pub family: String,
    pub version: i64,
    pub r#type: String,
    /// From the schema's `x-document-kind`.
    pub kind: String,
    /// The schema's `title`.
    pub description: String,
    /// The template's input contract (JSON Schema).
    pub data_schema: Json<serde_json::Value>,
    pub sample_data: Json<serde_json::Value>,
    pub status: String,
    pub scope: String,
    /// `.typ` path relative to `TEMPLATES_DIR`.
    pub file_path: String,
    pub sha256: String,
    pub min_app_version: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TemplateRow {
    /// Rows are only written by this service after validation, so a value
    /// that does not parse means the database was edited by hand.
    pub fn try_into_template(self) -> Result<Template, String> {
        let template_type = self.r#type.parse::<TemplateType>()?;
        let status = self.status.parse::<TemplateStatus>()?;
        let scope = self.scope.parse::<TemplateScope>()?;
        let options = parse_options(&self.data_schema.0)?;
        let is_default =
            self.data_schema.0.get("x-default") == Some(&serde_json::Value::Bool(true));

        Ok(Template {
            key: self.key,
            name: self.name,
            family: self.family,
            version: self.version,
            r#type: template_type,
            kind: self.kind,
            description: self.description,
            status,
            scope,
            is_default,
            options,
            data_schema: self.data_schema.0,
            data: self.sample_data.0,
            sha256: self.sha256,
            min_app_version: self.min_app_version,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }

    pub fn scope(&self) -> Result<TemplateScope, String> {
        self.scope.parse()
    }
}
