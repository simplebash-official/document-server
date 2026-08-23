// HTTP layer for the template-data module: CRUD over the per-template
// stored blobs. Every route is InternalCaller-gated — these blobs are
// business data (shop profiles, bank details), so unlike `templates`' read
// routes there is no open path here. The body of the PUT is the blob's JSON
// verbatim: any shape is accepted at write time, because whether a field
// matters is the *template's* schema's decision, enforced at render.

use axum::{
    Json,
    extract::{Path, State},
};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::modules,
        error::AppResult,
        middleware::auth::InternalCaller,
        response::{ApiResponse, ErrorResponse},
    },
    domain::template_data::{TemplateData, TemplateDataListResponse},
    modules::template_data::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_template_data))
        .routes(routes!(get_template_data))
        .routes(routes!(set_template_data))
        .routes(routes!(delete_template_data))
}

/// Request body for `PUT /{templateName}/{dataKey}` — documented for the
/// OpenAPI spec only; the handler accepts any JSON value.
#[derive(ToSchema)]
pub struct StoredDataBody(pub serde_json::Value);

// ============================================================================
// Template data
// ============================================================================

#[utoipa::path(
    put,
    path = "/{templateName}/{dataKey}",
    tag = modules::TEMPLATE_DATA,
    params(
        ("templateName" = String, Path, description = "The owning template's name (its `.typ` stem, e.g. 'a4-invoice')"),
        ("dataKey" = String, Path, description = "Caller-chosen name for this blob within the template's namespace"),
    ),
    request_body = StoredDataBody,
    responses(
        (status = 200, description = "Stored (or replaced)", body = ApiResponse<TemplateData>),
        (status = 401, description = "Missing or invalid X-Internal-Api-Key header", body = ErrorResponse),
        (status = 404, description = "Template not found", body = ErrorResponse),
    ),
    security(("internalApiKey" = []))
)]
async fn set_template_data(
    _internal: InternalCaller,
    State(state): State<AppState>,
    Path((template_name, data_key)): Path<(String, String)>,
    Json(data): Json<serde_json::Value>,
) -> AppResult<Json<ApiResponse<TemplateData>>> {
    let stored = service::set_template_data(&state.db, &template_name, &data_key, data).await?;

    Ok(Json(ApiResponse::success(stored, "Template data stored")))
}

#[utoipa::path(
    get,
    path = "/{templateName}/{dataKey}",
    tag = modules::TEMPLATE_DATA,
    params(
        ("templateName" = String, Path, description = "The owning template's name"),
        ("dataKey" = String, Path, description = "Name of the blob to fetch"),
    ),
    responses(
        (status = 200, description = "One stored blob", body = ApiResponse<TemplateData>),
        (status = 401, description = "Missing or invalid X-Internal-Api-Key header", body = ErrorResponse),
        (status = 404, description = "Blob not found", body = ErrorResponse),
    ),
    security(("internalApiKey" = []))
)]
async fn get_template_data(
    _internal: InternalCaller,
    State(state): State<AppState>,
    Path((template_name, data_key)): Path<(String, String)>,
) -> AppResult<Json<ApiResponse<TemplateData>>> {
    let stored = service::get_template_data(&state.db, &template_name, &data_key).await?;

    Ok(Json(ApiResponse::success(
        stored,
        "Template data retrieved successfully",
    )))
}

#[utoipa::path(
    get,
    path = "/{templateName}",
    tag = modules::TEMPLATE_DATA,
    params(
        ("templateName" = String, Path, description = "The owning template's name"),
    ),
    responses(
        (status = 200, description = "Every blob stored for that template", body = ApiResponse<TemplateDataListResponse>),
        (status = 401, description = "Missing or invalid X-Internal-Api-Key header", body = ErrorResponse),
    ),
    security(("internalApiKey" = []))
)]
async fn list_template_data(
    _internal: InternalCaller,
    State(state): State<AppState>,
    Path(template_name): Path<String>,
) -> AppResult<Json<ApiResponse<TemplateDataListResponse>>> {
    let response = service::list_template_data(&state.db, &template_name).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Template data retrieved successfully",
    )))
}

#[utoipa::path(
    delete,
    path = "/{templateName}/{dataKey}",
    tag = modules::TEMPLATE_DATA,
    params(
        ("templateName" = String, Path, description = "The owning template's name"),
        ("dataKey" = String, Path, description = "Name of the blob to delete"),
    ),
    responses(
        (status = 200, description = "Deleted — the removed blob is echoed back", body = ApiResponse<TemplateData>),
        (status = 401, description = "Missing or invalid X-Internal-Api-Key header", body = ErrorResponse),
        (status = 404, description = "Blob not found", body = ErrorResponse),
    ),
    security(("internalApiKey" = []))
)]
async fn delete_template_data(
    _internal: InternalCaller,
    State(state): State<AppState>,
    Path((template_name, data_key)): Path<(String, String)>,
) -> AppResult<Json<ApiResponse<TemplateData>>> {
    let deleted = service::delete_template_data(&state.db, &template_name, &data_key).await?;

    Ok(Json(ApiResponse::success(deleted, "Template data deleted")))
}
