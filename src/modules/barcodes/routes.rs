// HTTP endpoints for the barcodes module.

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
        barcodes::{BarcodeResponse, BarcodeSymbology, GenerateBarcodeRequest},
    },
    modules::barcodes::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(status))
        .routes(routes!(generate_barcode_post))
        .routes(routes!(generate_barcode_svg_get))
}

// ============================================================================
// Module status
// ============================================================================

#[utoipa::path(get, path = "/", tag = modules::BARCODES, responses(
    (status = 200, description = "Barcodes module status", body = ApiResponse<ModuleStatusResponse>)
))]
async fn status() -> Json<ApiResponse<ModuleStatusResponse>> {
    module_status_response(modules::BARCODES)
}

// ============================================================================
// Generate Barcode
// ============================================================================

#[utoipa::path(
    post,
    path = "/generate",
    tag = modules::BARCODES,
    request_body = GenerateBarcodeRequest,
    responses(
        (status = 200, description = "Generated barcode metadata and SVG", body = ApiResponse<BarcodeResponse>),
        (status = 400, description = "Invalid payload or unsupported symbology", body = crate::core::response::ErrorResponse),
    )
)]
async fn generate_barcode_post(
    Json(payload): Json<GenerateBarcodeRequest>,
) -> AppResult<Json<ApiResponse<BarcodeResponse>>> {
    let result = service::generate_barcode(&payload)?;
    Ok(Json(ApiResponse::success(
        result,
        "Barcode generated successfully",
    )))
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct BarcodeQueryParams {
    /// Content string to encode in barcode
    pub content: String,
    /// Barcode symbology (code128, code39, ean13, ean8). Defaults to code128.
    pub symbology: Option<String>,
    /// Height of barcode (defaults to 80)
    pub height: Option<u32>,
}

#[utoipa::path(
    get,
    path = "/generate",
    tag = modules::BARCODES,
    params(BarcodeQueryParams),
    responses(
        (status = 200, description = "Generated SVG barcode image", content_type = "image/svg+xml", body = String),
        (status = 400, description = "Invalid payload or unsupported symbology", body = crate::core::response::ErrorResponse),
    )
)]
async fn generate_barcode_svg_get(Query(params): Query<BarcodeQueryParams>) -> AppResult<Response> {
    let symbology = if let Some(sym_str) = params.symbology {
        sym_str.parse().unwrap_or(BarcodeSymbology::Code128)
    } else {
        BarcodeSymbology::Code128
    };

    let req = GenerateBarcodeRequest {
        content: params.content,
        symbology,
        height: params.height,
    };

    let result = service::generate_barcode(&req)?;

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/svg+xml")],
        result.svg,
    )
        .into_response())
}
