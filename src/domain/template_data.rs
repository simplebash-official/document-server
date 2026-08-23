// Pure business types for the template-data feature — no I/O, no
// SQLite/Axum types beyond serde/utoipa derives.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One stored blob of shared/static input data, scoped to a template by its
/// *name* (the disk identity — keys are stable but names are what a caller
/// resolves before rendering). `data_key` is the caller-chosen name of the
/// blob within that template's namespace (e.g. `shop`, `branding`); at
/// render time every blob for the template is deep-merged under the request
/// payload, with request fields winning.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TemplateData {
    pub key: String,
    pub template_name: String,
    pub data_key: String,
    pub data: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Response body for `GET /api/template-data/{templateName}`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TemplateDataListResponse {
    pub items: Vec<TemplateData>,
}
