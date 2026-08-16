// Pure business types shared across modules — no I/O, no framework types
// beyond serde/utoipa derives.

pub mod barcodes;
pub mod documents;
pub mod qrcodes;
pub mod templates;

use serde::Serialize;
use utoipa::ToSchema;

/// Payload for the top-level `/health` liveness check (see `app.rs`).
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
}

/// Payload every feature module's placeholder `GET /` status route returns
/// (built via `core::utils::module_status_response`) — a trivial "is this
/// module wired up" check, not a health/readiness probe.
#[derive(Debug, Serialize, ToSchema)]
pub struct ModuleStatusResponse {
    pub module: String,
    pub status: String,
}
