// Pure business types shared across modules — no I/O, no framework types
// beyond serde/utoipa derives.

pub mod barcodes;
pub mod documents;
pub mod qrcodes;
pub mod template_data;
pub mod templates;

use serde::Serialize;
use utoipa::ToSchema;

/// Payload for the top-level `/health` liveness check (see `app.rs`).
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    /// Service uptime in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_seconds: Option<u64>,
    /// Environment (e.g. "development" or "production").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    /// Application package version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Payload every feature module's placeholder `GET /` status route returns
/// (built via `core::utils::module_status_response`) — a trivial "is this
/// module wired up" check, not a health/readiness probe.
#[derive(Debug, Serialize, ToSchema)]
pub struct ModuleStatusResponse {
    pub module: String,
    pub status: String,
}
