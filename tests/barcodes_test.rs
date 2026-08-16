mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn generate_barcode_post_success() {
    let app = common::spawn_app().await;

    let payload = json!({
        "content": "INV-123456",
        "symbology": "code128",
        "height": 60
    });

    let response = app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/barcodes/generate")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["content"], "INV-123456");
    assert_eq!(json["data"]["symbology"], "code128");
    assert!(json["data"]["svg"].as_str().unwrap().contains("<svg"));
}

#[tokio::test]
async fn generate_barcode_get_svg_stream() {
    let app = common::spawn_app().await;

    let response = app
        .router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/barcodes/generate?content=SKU-9900&symbology=code39&height=70")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("image/svg+xml")
    );

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(body_str.contains("<svg"));
}

#[tokio::test]
async fn generate_barcode_empty_content_returns_400() {
    let app = common::spawn_app().await;

    let payload = json!({
        "content": "   ",
        "symbology": "code128"
    });

    let response = app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/barcodes/generate")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["code"], "VALIDATION_ERROR");
}
