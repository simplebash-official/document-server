// HTTP layer for the templates module. `GET /` (list) and `GET /{key}`
// (single template's metadata/data-contract) are both real as of Phase 2.
// Unlike `render`/`documents`, there's no placeholder module-status stub
// here — the collection root is itself the real "list templates" endpoint
// from day one, so there's no unused path left for one (same convention
// jana2u-pos's `suppliers` module uses).

use axum::{
    Json,
    extract::{Path, State},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::modules,
        error::AppResult,
        response::{ApiResponse, ErrorResponse},
    },
    domain::templates::{Template, TemplatesResponse},
    modules::templates::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_templates))
        .routes(routes!(get_template))
}

// ============================================================================
// Templates
// ============================================================================

#[utoipa::path(get, path = "/", tag = modules::TEMPLATES, responses(
    (status = 200, description = "List templates (active and inactive)", body = ApiResponse<TemplatesResponse>)
))]
async fn list_templates(
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<TemplatesResponse>>> {
    let response = service::list_templates(&state.db).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Templates retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/{key}", tag = modules::TEMPLATES,
    params(("key" = String, Path, description = "The template's key (tpl_...)")),
    responses(
        (status = 200, description = "Get a template", body = ApiResponse<Template>),
        (status = 404, description = "Template not found", body = ErrorResponse),
    )
)]
async fn get_template(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<Template>>> {
    let template = service::get_template_by_key(&state.db, &key).await?;

    Ok(Json(ApiResponse::success(
        template,
        "Template retrieved successfully",
    )))
}
