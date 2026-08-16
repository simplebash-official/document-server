// HTTP endpoints for the QR codes module.

use axum::{
    Json,
    extract::Query,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::modules, error::AppResult, response::ApiResponse, utils::module_status_response,
    },
    domain::{
        ModuleStatusResponse,
        qrcodes::{GenerateQrCodeRequest, QrCodeResponse, QrEccLevel},
    },
    modules::qrcodes::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(status))
        .routes(routes!(generate_qr_code_post))
        .routes(routes!(generate_qr_code_svg_get))
}

// ============================================================================
// Module status
// ============================================================================

#[utoipa::path(get, path = "/", tag = modules::QRCODES, responses(
    (status = 200, description = "QR Codes module status", body = ApiResponse<ModuleStatusResponse>)
))]
async fn status() -> Json<ApiResponse<ModuleStatusResponse>> {
    module_status_response(modules::QRCODES)
}

// ============================================================================
// Generate QR Code
// ============================================================================

#[utoipa::path(
    post,
    path = "/generate",
    tag = modules::QRCODES,
    request_body = GenerateQrCodeRequest,
    responses(
        (status = 200, description = "Generated QR code metadata and SVG", body = ApiResponse<QrCodeResponse>),
        (status = 400, description = "Invalid payload or generation error", body = crate::core::response::ErrorResponse),
    )
)]
async fn generate_qr_code_post(
    Json(payload): Json<GenerateQrCodeRequest>,
) -> AppResult<Json<ApiResponse<QrCodeResponse>>> {
    let result = service::generate_qr_code(&payload)?;
    Ok(Json(ApiResponse::success(
        result,
        "QR code generated successfully",
    )))
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct QrCodeQueryParams {
    /// Content string or URL to encode in QR code
    pub content: String,
    /// Error correction level (L, M, Q, H). Defaults to M.
    pub ecc: Option<String>,
}

#[utoipa::path(
    get,
    path = "/generate",
    tag = modules::QRCODES,
    params(QrCodeQueryParams),
    responses(
        (status = 200, description = "Generated SVG QR code image", content_type = "image/svg+xml", body = String),
        (status = 400, description = "Invalid payload or generation error", body = crate::core::response::ErrorResponse),
    )
)]
async fn generate_qr_code_svg_get(Query(params): Query<QrCodeQueryParams>) -> AppResult<Response> {
    let ecc = if let Some(ecc_str) = params.ecc {
        ecc_str.parse().unwrap_or(QrEccLevel::M)
    } else {
        QrEccLevel::M
    };

    let req = GenerateQrCodeRequest {
        content: params.content,
        ecc,
    };

    let result = service::generate_qr_code(&req)?;

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/svg+xml")],
        result.svg,
    )
        .into_response())
}
