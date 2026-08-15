// HTTP layer for the documents module. Phase 1 exposes only a module-status
// stub — real read routes (`GET /`, `GET /{key}`) are Phase 3 work (see
// spec §12); `service`/`repository` are already fully real in Phase 1
// because `modules::render` writes through them on every successful render.
// A route missing from the OpenAPI spec today is expected, not a bug — see
// CLAUDE.md.

use axum::Json;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{constants::modules, response::ApiResponse, utils::module_status_response},
    domain::ModuleStatusResponse,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(status))
}

// ============================================================================
// Module status
// ============================================================================

#[utoipa::path(get, path = "/", tag = modules::DOCUMENTS, responses(
    (status = 200, description = "Documents module status", body = ApiResponse<ModuleStatusResponse>)
))]
async fn status() -> Json<ApiResponse<ModuleStatusResponse>> {
    module_status_response(modules::DOCUMENTS)
}
