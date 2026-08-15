// HTTP layer for the render module. Every handler here follows the usual
// shape — extract params, make exactly one `service::*` call — except
// `render_template` itself, which is the one deliberate deviation from the
// rest of this service's response envelope (see spec §4): its success
// payload is a raw PDF byte body, not `Json<ApiResponse<_>>`. Its errors
// still go through the standard `AppError`/`ErrorResponse` path — only the
// success shape differs. Do not "fix" this to return `ApiResponse` — see
// CLAUDE.md.

use axum::{
    Json,
    body::Body,
    extract::{DefaultBodyLimit, Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::modules, error::AppResult, response::ApiResponse, utils::module_status_response,
    },
    domain::ModuleStatusResponse,
    modules::render::service,
};

// ============================================================================
// Router
// ============================================================================

/// `max_render_body_bytes` comes from `Config` (see spec §8/§9) — passed in
/// rather than read from a global, since `AppState`/`Config` don't exist yet
/// at router-construction time. Applied to the whole module's router (not
/// just the render route) for simplicity; it's a no-op for the bodyless
/// `GET /` status route.
pub fn router(max_render_body_bytes: usize) -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(status))
        .routes(routes!(render_template))
        .route_layer(DefaultBodyLimit::max(max_render_body_bytes))
}

// ============================================================================
// Module status
// ============================================================================

#[utoipa::path(get, path = "/", tag = modules::RENDER, responses(
    (status = 200, description = "Render module status", body = ApiResponse<ModuleStatusResponse>)
))]
async fn status() -> Json<ApiResponse<ModuleStatusResponse>> {
    module_status_response(modules::RENDER)
}

// ============================================================================
// Render
// ============================================================================

#[utoipa::path(
    post,
    path = "/{templateKey}",
    tag = modules::RENDER,
    params(("templateKey" = String, Path, description = "The templates.key (tpl_...) to render")),
    request_body(
        content = serde_json::Value,
        description = "Arbitrary JSON payload matching the target template's documented data contract (see the .typ file's header comment)"
    ),
    responses(
        (status = 200, description = "Rendered PDF", content_type = "application/pdf", body = Vec<u8>),
        (status = 404, description = "Unknown or inactive template", body = crate::core::response::ErrorResponse),
        (status = 422, description = "Request data failed template compilation", body = crate::core::response::ErrorResponse),
        (status = 500, description = "PDF export failed", body = crate::core::response::ErrorResponse),
    )
)]
async fn render_template(
    State(state): State<AppState>,
    Path(template_key): Path<String>,
    Json(data): Json<serde_json::Value>,
) -> AppResult<Response> {
    let (pdf_bytes, _) =
        service::render_template(&state.db, &state.render, &template_key, data).await?;

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/pdf")],
        Body::from(pdf_bytes),
    )
        .into_response())
}
