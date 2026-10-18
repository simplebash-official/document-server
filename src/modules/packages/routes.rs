// HTTP layer for template packages. Every route needs the internal API key.
// Package bytes travel as a raw `application/zip` body (not JSON, not
// multipart); responses use the standard envelope except the archive and the
// preview, which are raw bytes like `POST /api/render/{key}`.

use axum::{
    Json,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::modules,
        error::AppResult,
        middleware::auth::InternalCaller,
        response::{ApiResponse, ErrorResponse},
    },
    domain::packages::{GrantsRequest, PackageInfo, PackagesResponse, RetireResponse},
    modules::packages::service,
};

pub fn router(max_package_bytes: usize) -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_packages, publish_package))
        .routes(routes!(get_package, install_package))
        .routes(routes!(package_archive))
        .routes(routes!(package_preview))
        .routes(routes!(set_grants))
        .routes(routes!(retire_family))
        .route_layer(DefaultBodyLimit::max(max_package_bytes))
}

#[utoipa::path(
    get, path = "/", tag = modules::PACKAGES,
    responses(
        (status = 200, description = "Every template and package, all statuses, with grants", body = ApiResponse<PackagesResponse>),
        (status = 401, description = "Missing or invalid X-Internal-Api-Key header", body = ErrorResponse),
    ),
    security(("internalApiKey" = []))
)]
async fn list_packages(
    _internal: InternalCaller,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<PackagesResponse>>> {
    Ok(Json(ApiResponse::success(
        service::list(&state.db).await?,
        "Packages retrieved successfully",
    )))
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct PublishQuery {
    /// Publish as the next version of this family; omit to start a new design.
    family: Option<String>,
}

#[utoipa::path(
    post, path = "/", tag = modules::PACKAGES,
    params(PublishQuery),
    request_body(content = Vec<u8>, content_type = "application/zip", description = "manifest.json, template.typ, schema.json, sample.json and optional assets/"),
    responses(
        (status = 201, description = "Published as a new immutable version", body = ApiResponse<PackageInfo>),
        (status = 401, description = "Missing or invalid X-Internal-Api-Key header", body = ErrorResponse),
        (status = 409, description = "Another publish took the same name or version", body = ErrorResponse),
        (status = 413, description = "Archive larger than MAX_PACKAGE_BYTES"),
        (status = 422, description = "The archive is not a valid package, or it does not render its sample", body = ErrorResponse),
    ),
    security(("internalApiKey" = []))
)]
async fn publish_package(
    _internal: InternalCaller,
    State(state): State<AppState>,
    Query(query): Query<PublishQuery>,
    body: Bytes,
) -> AppResult<(StatusCode, Json<ApiResponse<PackageInfo>>)> {
    let info = service::publish(
        &state.db,
        &state.render,
        &state.config,
        &body,
        query.family.as_deref(),
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(info, "Package published")),
    ))
}

#[utoipa::path(
    get, path = "/{name}", tag = modules::PACKAGES,
    params(("name" = String, Path, description = "The template name (doc_temp_... / lbl_temp_...)")),
    responses(
        (status = 200, description = "One template or package with its grants", body = ApiResponse<PackageInfo>),
        (status = 401, description = "Missing or invalid X-Internal-Api-Key header", body = ErrorResponse),
        (status = 404, description = "Unknown name", body = ErrorResponse),
    ),
    security(("internalApiKey" = []))
)]
async fn get_package(
    _internal: InternalCaller,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<Json<ApiResponse<PackageInfo>>> {
    Ok(Json(ApiResponse::success(
        service::get(&state.db, &name).await?,
        "Package retrieved successfully",
    )))
}

#[derive(Debug, Deserialize, IntoParams)]
struct InstallQuery {
    /// Hex SHA-256 of the archive being uploaded.
    sha256: String,
}

#[utoipa::path(
    put, path = "/{name}", tag = modules::PACKAGES,
    params(("name" = String, Path, description = "The template name the archive's manifest names"), InstallQuery),
    request_body(content = Vec<u8>, content_type = "application/zip", description = "An archive exported by this service (its manifest carries the identity)"),
    responses(
        (status = 200, description = "Already installed with identical content; family (re)granted to the caller's tenant", body = ApiResponse<PackageInfo>),
        (status = 201, description = "Installed; family granted to the caller's tenant", body = ApiResponse<PackageInfo>),
        (status = 401, description = "Missing or invalid X-Internal-Api-Key header", body = ErrorResponse),
        (status = 409, description = "Different content already installed under this name", body = ErrorResponse),
        (status = 413, description = "Archive larger than MAX_PACKAGE_BYTES"),
        (status = 422, description = "Invalid archive, identity mismatch or sha256 mismatch", body = ErrorResponse),
    ),
    security(("internalApiKey" = []))
)]
async fn install_package(
    internal: InternalCaller,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<InstallQuery>,
    body: Bytes,
) -> AppResult<(StatusCode, Json<ApiResponse<PackageInfo>>)> {
    let (info, created) = service::install(
        &state.db,
        &state.render,
        &state.config,
        &name,
        &query.sha256,
        &body,
        internal.tenant_key.as_column(),
    )
    .await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        Json(ApiResponse::success(info, "Package installed")),
    ))
}

#[utoipa::path(
    get, path = "/{name}/archive", tag = modules::PACKAGES,
    params(("name" = String, Path, description = "The template name")),
    responses(
        (status = 200, description = "The package archive", content_type = "application/zip", body = Vec<u8>),
        (status = 401, description = "Missing or invalid X-Internal-Api-Key header", body = ErrorResponse),
        (status = 404, description = "Not a private package the calling tenant may use", body = ErrorResponse),
    ),
    security(("internalApiKey" = []))
)]
async fn package_archive(
    internal: InternalCaller,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<Response> {
    let bytes = service::archive_bytes(
        &state.db,
        &state.config,
        internal.tenant_key.as_column(),
        &name,
    )
    .await?;
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/zip")],
        Body::from(bytes),
    )
        .into_response())
}

#[utoipa::path(
    post, path = "/{name}/preview", tag = modules::PACKAGES,
    params(("name" = String, Path, description = "The template name")),
    responses(
        (status = 200, description = "The template rendered with its own sample", content_type = "application/pdf", body = Vec<u8>),
        (status = 401, description = "Missing or invalid X-Internal-Api-Key header", body = ErrorResponse),
        (status = 404, description = "Unknown name", body = ErrorResponse),
        (status = 422, description = "The sample no longer renders", body = ErrorResponse),
    ),
    security(("internalApiKey" = []))
)]
async fn package_preview(
    _internal: InternalCaller,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<Response> {
    let bytes = service::preview(&state.db, &state.render, &state.config, &name).await?;
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/pdf")],
        Body::from(bytes),
    )
        .into_response())
}

#[utoipa::path(
    put, path = "/families/{family}/grants", tag = modules::PACKAGES,
    params(("family" = String, Path, description = "The family id (fam_...)")),
    request_body = GrantsRequest,
    responses(
        (status = 204, description = "Grants replaced"),
        (status = 400, description = "Invalid tenant key, or a bundled family", body = ErrorResponse),
        (status = 401, description = "Missing or invalid X-Internal-Api-Key header", body = ErrorResponse),
        (status = 404, description = "Unknown family", body = ErrorResponse),
    ),
    security(("internalApiKey" = []))
)]
async fn set_grants(
    _internal: InternalCaller,
    State(state): State<AppState>,
    Path(family): Path<String>,
    Json(body): Json<GrantsRequest>,
) -> AppResult<StatusCode> {
    service::set_grants(&state.db, &family, &body.tenant_keys).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post, path = "/families/{family}/retire", tag = modules::PACKAGES,
    params(("family" = String, Path, description = "The family id (fam_...)")),
    responses(
        (status = 200, description = "Every published version retired; documents already produced still reprint", body = ApiResponse<RetireResponse>),
        (status = 400, description = "A bundled family", body = ErrorResponse),
        (status = 401, description = "Missing or invalid X-Internal-Api-Key header", body = ErrorResponse),
        (status = 404, description = "Unknown family", body = ErrorResponse),
    ),
    security(("internalApiKey" = []))
)]
async fn retire_family(
    _internal: InternalCaller,
    State(state): State<AppState>,
    Path(family): Path<String>,
) -> AppResult<Json<ApiResponse<RetireResponse>>> {
    let retired_versions = service::retire(&state.db, &family).await?;
    Ok(Json(ApiResponse::success(
        RetireResponse { retired_versions },
        "Family retired",
    )))
}
