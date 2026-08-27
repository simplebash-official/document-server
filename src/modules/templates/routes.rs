// HTTP layer for the templates module. `GET /` (list) and `GET /{key}`
// (single template's metadata/data-contract), plus `POST /sync` — the
// runtime disk re-sync that lets a new or edited `.typ` go live without a
// process restart.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
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
    domain::templates::{
        CreateTemplateRequest, SyncTemplatesResponse, Template, TemplateType, TemplatesResponse,
    },
    modules::templates::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_templates, create_template))
        .routes(routes!(get_template))
        .routes(routes!(sync_templates))
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

/// Publishes a new template from an API call instead of a shell session:
/// the server mints its `<type>_temp_<nanoid>` identity, writes the `.typ`
/// (and any schema/sample sidecars) into `templates/documents|labels/`,
/// re-syncs, and hot-swaps the engine — the new template renders on the
/// very next request, no restart. Internal-caller-gated for the same reason
/// as `/sync`: it rebuilds the engine every other caller depends on.
#[utoipa::path(
    post,
    path = "/",
    tag = modules::TEMPLATES,
    request_body = CreateTemplateRequest,
    responses(
        (status = 201, description = "Template created and live", body = ApiResponse<Template>),
        (status = 401, description = "Missing or invalid X-Internal-Api-Key header", body = ErrorResponse),
        (status = 422, description = "Empty source, invalid schema, or the template does not compile", body = ErrorResponse),
        (status = 500, description = "Could not write files or rebuild the engine", body = ErrorResponse),
    ),
    security(("internalApiKey" = []))
)]
async fn create_template(
    _internal: InternalCaller,
    State(state): State<AppState>,
    Json(req): Json<CreateTemplateRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Template>>)> {
    let (engine, template) = service::create_template(&state.db, &state.config, req).await?;

    // Swap only after the whole create succeeded — same discipline as `/sync`.
    state.render.store(Arc::new(engine));

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(template, "Template created")),
    ))
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

// ============================================================================
// Sync
// ============================================================================

/// Re-syncs templates from disk without a restart: rebuilds the render
/// engine, upserts metadata for every `.typ` found, and deactivates rows
/// whose file disappeared (never deletes — keys are stable and historical
/// documents reference them). Internal-caller-gated like `render`, since it
/// swaps the engine every caller of this service depends on.
#[utoipa::path(
    post,
    path = "/sync",
    tag = modules::TEMPLATES,
    responses(
        (status = 200, description = "Templates re-synced from disk", body = ApiResponse<SyncTemplatesResponse>),
        (status = 401, description = "Missing or invalid X-Internal-Api-Key header", body = ErrorResponse),
        (status = 500, description = "Templates directory unreadable or database failure", body = ErrorResponse),
    ),
    security(("internalApiKey" = []))
)]
async fn sync_templates(
    _internal: InternalCaller,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<SyncTemplatesResponse>>> {
    let (engine, summary) = service::resync_from_disk(
        &state.db,
        &state.config.templates_dir,
        &state.config.fonts_dir,
    )
    .await?;

    // Swapped only after the whole sync succeeded — a failed warm-up leaves
    // the previous engine serving untouched.
    state.render.store(Arc::new(engine));

    Ok(Json(ApiResponse::success(
        summary,
        "Templates re-synced from disk",
    )))
}
