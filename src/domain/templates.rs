// Pure business types for the templates feature — no I/O, no SQLite/Axum
// types beyond serde/utoipa derives. SQLite row shapes live in
// `modules::templates::model` and convert into these before a handler wraps
// them in `core::response::ApiResponse<T>`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A template as returned to API clients. `key` (see `core::id::generate_id`)
/// is the only external identifier and is also the table's primary key —
/// there is no separate autoincrement/row id to accidentally leak.
/// `name` matches the `.typ` filename stem on disk (the template file itself
/// is the rendering contract; `data_schema` here is a queryable *description*
/// of that same contract, not a second source of truth — see spec §6.1).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub key: String,
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_schema: Option<serde_json::Value>,
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
