// HTTP layer for the templates module: `GET /` lists the latest published
// version of every design the calling tenant may use, `GET /{key}` returns
// one template's metadata and data contract. Both need the internal API key
// and are scoped by `X-Tenant-Key`: a private template is invisible to a
// tenant it was not granted to. Publishing and managing templates lives in
// `modules::packages`.

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
        middleware::auth::InternalCaller,
        response::{ApiResponse, ErrorResponse},
    },
    domain::templates::{Template, TemplateType, TemplatesResponse},
    modules::templates::service,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_templates))
        .routes(routes!(get_template))
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct ListTemplatesQuery {
    /// Filter templates by type: "document" or "label"
    pub r#type: Option<TemplateType>,
}

#[utoipa::path(
    get,
    path = "/",
    tag = modules::TEMPLATES,
    params(ListTemplatesQuery),
    responses(
        (status = 200, description = "Latest published version of each design the tenant may use", body = ApiResponse<TemplatesResponse>),
        (status = 400, description = "Unknown `type` filter", body = ErrorResponse),
        (status = 401, description = "Missing or invalid X-Internal-Api-Key header", body = ErrorResponse),
    ),
    security(("internalApiKey" = []))
)]
async fn list_templates(
    internal: InternalCaller,
    State(state): State<AppState>,
    Query(query): Query<ListTemplatesQuery>,
) -> AppResult<Json<ApiResponse<TemplatesResponse>>> {
    let response =
        service::list_templates(&state.db, internal.tenant_key.as_column(), query.r#type).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Templates retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/{key}", tag = modules::TEMPLATES,
    params(("key" = String, Path, description = "The template's key (tpl_...)")),
    responses(
        (status = 200, description = "Get a template", body = ApiResponse<Template>),
        (status = 401, description = "Missing or invalid X-Internal-Api-Key header", body = ErrorResponse),
        (status = 404, description = "Template not found, retired, or not visible to the tenant", body = ErrorResponse),
    ),
    security(("internalApiKey" = []))
)]
async fn get_template(
    internal: InternalCaller,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<Template>>> {
    let template =
        service::get_template_by_key(&state.db, internal.tenant_key.as_column(), &key).await?;

    Ok(Json(ApiResponse::success(
        template,
        "Template retrieved successfully",
    )))
}
