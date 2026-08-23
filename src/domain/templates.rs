// Pure business types for the templates feature — no I/O, no SQLite/Axum
// types beyond serde/utoipa derives. SQLite row shapes live in
// `modules::templates::model` and convert into these before a handler wraps
// them in `core::response::ApiResponse<T>`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Category of template: standard printable `document` (receipt, invoice, report)
/// or a `label` (thermal sticker, product tag, barcode label).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "camelCase")]
pub enum TemplateType {
    #[default]
    Document,
    Label,
}

impl std::fmt::Display for TemplateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Document => write!(f, "document"),
            Self::Label => write!(f, "label"),
        }
    }
}

impl std::str::FromStr for TemplateType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "document" | "documents" => Ok(Self::Document),
            "label" | "labels" => Ok(Self::Label),
            other => Err(format!("unknown template type: {other}")),
        }
    }
}

/// A template as returned to API clients. `key` (see `core::id::generate_id`)
/// is the only external identifier and is also the table's primary key —
/// there is no separate autoincrement/row id to accidentally leak.
/// `name` matches the `.typ` filename stem or path on disk (the template file itself
/// is the rendering contract; `data_schema` here is a queryable *description*
/// of that same contract, not a second source of truth — see spec §6.1).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub key: String,
    pub name: String,
    pub r#type: TemplateType,
    pub description: String,
    /// The template's machine-readable input contract (JSON Schema, parsed
    /// from the `<name>.schema.json` sidecar) — what an integration client
    /// fetches in order to know how to feed a render. `None` when the
    /// template ships no schema sidecar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_schema: Option<serde_json::Value>,
    /// A worked example of valid input (parsed from the `<name>.json`
    /// sidecar) — documentation/sample payload, never enforced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Response body for `GET /api/templates`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TemplatesResponse {
    pub templates: Vec<Template>,
}

/// Response body for `POST /api/templates/sync` — what the disk scan found,
/// and how many stale rows (`.typ` files that no longer exist on disk) were
/// marked inactive. Template names are included so an operator immediately
/// sees what the service now knows about.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SyncTemplatesResponse {
    /// Templates discovered on disk and upserted (active).
    pub synced_count: u64,
    /// Previously-active rows whose `.typ` file is gone from disk — they are
    /// deactivated, never deleted (their keys stay stable for history).
    pub deactivated_count: u64,
    pub templates: Vec<String>,
}
