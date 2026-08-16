mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn generate_qr_code_post_success() {
    let app = common::spawn_app().await;

    let payload = json!({
        "content": "https://jana2u.com/pay/inv_9988",
        "ecc": "M"
    });

    let response = app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/qrcodes/generate")
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
    assert_eq!(json["data"]["content"], "https://jana2u.com/pay/inv_9988");
    assert_eq!(json["data"]["ecc"], "M");
    assert!(json["data"]["svg"].as_str().unwrap().contains("<svg"));
}

#[tokio::test]
async fn generate_qr_code_get_svg_stream() {
    let app = common::spawn_app().await;

    let response = app
        .router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/qrcodes/generate?content=HELLO_WORLD&ecc=H")
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
async fn generate_qr_code_empty_content_returns_400() {
    let app = common::spawn_app().await;

    let payload = json!({
        "content": "   ",
        "ecc": "M"
    });

    let response = app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/qrcodes/generate")
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
