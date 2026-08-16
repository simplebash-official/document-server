// HTTP layer for the templates module. `GET /` (list) and `GET /{key}`
// (single template's metadata/data-contract).

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::modules,
        error::AppResult,
        response::{ApiResponse, ErrorResponse},
    },
    domain::templates::{Template, TemplateType, TemplatesResponse},
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

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct ListTemplatesQuery {
    /// Filter templates by type: "document" or "label"
    pub r#type: Option<String>,
}

#[utoipa::path(
    get,
    path = "/",
    tag = modules::TEMPLATES,
    params(ListTemplatesQuery),
    responses(
        (status = 200, description = "List templates (active and inactive)", body = ApiResponse<TemplatesResponse>)
    )
)]
async fn list_templates(
    State(state): State<AppState>,
    Query(query): Query<ListTemplatesQuery>,
) -> AppResult<Json<ApiResponse<TemplatesResponse>>> {
    let type_filter = query.r#type.and_then(|t| t.parse::<TemplateType>().ok());
    let response = service::list_templates(&state.db, type_filter).await?;

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
