// Pure business types for the documents feature — no I/O, no SQLite/Axum
// types beyond serde/utoipa derives. SQLite row shapes live in
// `modules::documents::model` and convert into these before a handler wraps
// them in `core::response::ApiResponse<T>`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Record of one successful render. Stores `data` — the exact JSON payload
/// the request was rendered with — rather than the PDF bytes themselves, so
/// a reprint always reflects the *current* template and no blob storage is
/// needed (see spec §6.2). `template_key` is a foreign key by `key`, never
/// by name or row id — same rule every cross-table reference in this
/// service follows.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub key: String,
    pub template_key: String,
    pub data: serde_json::Value,
    pub file_size_bytes: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
