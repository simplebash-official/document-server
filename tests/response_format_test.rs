// No router, no database — calls `ApiResponse`/`AppError` directly and
// asserts the JSON envelope shape. The fastest/narrowest of the four test
// tiers; use this style for anything about the response/error envelope
// itself, not endpoint behavior.

use axum::{http::StatusCode, response::IntoResponse};
use document_server::core::{error::AppError, response::ApiResponse};
use serde_json::json;

#[tokio::test]
async fn test_success_response_format() {
    let response = ApiResponse::success(
        json!({"key": "tpl_123", "name": "receipt"}),
        "Template retrieved successfully",
    )
    .into_response();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["key"], "tpl_123");
    assert_eq!(json["data"]["name"], "receipt");
    assert_eq!(json["message"], "Template retrieved successfully");
    assert!(
        json.get("processingTimeMs").is_none(),
        "document_server has no timing middleware — processingTimeMs should never appear"
    );
}

#[tokio::test]
async fn test_error_response_format() {
    let err = AppError::not_found_with_code("Template not found", "TEMPLATE_NOT_FOUND");
    let response = err.into_response();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["success"], false);
    assert_eq!(json["message"], "Template not found");
    assert_eq!(json["code"], "TEMPLATE_NOT_FOUND");
    assert_eq!(json["statusCode"], 404);
}

#[tokio::test]
async fn test_standard_app_error_defaults() {
    let err = AppError::not_found("Item missing");
    let response = err.into_response();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["success"], false);
    assert_eq!(json["message"], "Item missing");
    assert_eq!(json["code"], "NOT_FOUND");
    assert_eq!(json["statusCode"], 404);
}

/// Regression guard for the one deliberate deviation from SimpleBash POS's
/// `AppError` status mapping this service makes: `unprocessable_entity`
/// (used by `modules::render::service` for a Typst compile failure) must
/// produce 422, not `Validation`'s normal 400 — see spec §4/§6.2 and
/// `core::error::AppError::unprocessable_entity`.
#[tokio::test]
async fn test_unprocessable_entity_is_422() {
    let err = AppError::unprocessable_entity("RENDER_VALIDATION_FAILED", "bad input");
    let response = err.into_response();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["success"], false);
    assert_eq!(json["code"], "RENDER_VALIDATION_FAILED");
    assert_eq!(json["statusCode"], 422);
}
