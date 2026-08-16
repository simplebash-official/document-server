// Full-stack, real SQLite (throwaway per-test file via `common::spawn_app`).
// Covers the real `GET /api/templates` / `GET /api/templates/{key}` read
// routes with template type categorization (documents vs labels) and
// expected `data` sub-objects.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn list_templates_returns_every_seeded_template_with_type_and_expected_data() {
    let app = common::spawn_app().await;

    let response = app
        .router
        .oneshot(
            Request::builder()
                .uri("/api/templates")
                .body(Body::empty())
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

    let templates = json["data"]["templates"].as_array().unwrap();
    let names: Vec<&str> = templates
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["receipt", "sticker"]);
    assert!(templates.iter().all(|t| t["isActive"] == true));

    let receipt_tpl = templates.iter().find(|t| t["name"] == "receipt").unwrap();
    assert_eq!(receipt_tpl["type"], "document");
    assert_eq!(receipt_tpl["data"]["invoiceNumber"], "PPmay056");
    assert!(receipt_tpl["data"]["items"].is_array());
    assert_eq!(receipt_tpl["data"]["total"], 157.08);
    assert_eq!(receipt_tpl["data"]["footerCode"]["type"], "barcode");

    let sticker_tpl = templates.iter().find(|t| t["name"] == "sticker").unwrap();
    assert_eq!(sticker_tpl["type"], "label");
    assert_eq!(sticker_tpl["data"]["title"], "USB-C Cable");
    assert_eq!(sticker_tpl["data"]["reference"], "SKU-00042");
}

#[tokio::test]
async fn list_templates_filters_by_type() {
    let app = common::spawn_app().await;

    // Filter by type=document
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/templates?type=document")
                .body(Body::empty())
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

    let templates = json["data"]["templates"].as_array().unwrap();
    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0]["name"], "receipt");
    assert_eq!(templates[0]["type"], "document");
    assert_eq!(templates[0]["data"]["invoiceNumber"], "PPmay056");

    // Filter by type=label
    let response = app
        .router
        .oneshot(
            Request::builder()
                .uri("/api/templates?type=label")
                .body(Body::empty())
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

    let templates = json["data"]["templates"].as_array().unwrap();
    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0]["name"], "sticker");
    assert_eq!(templates[0]["type"], "label");
    assert_eq!(templates[0]["data"]["reference"], "SKU-00042");
}

#[tokio::test]
async fn get_template_by_key_returns_that_template_with_data() {
    let app = common::spawn_app().await;

    let sticker_key: String = sqlx::query_scalar("SELECT key FROM templates WHERE name = ?")
        .bind("sticker")
        .fetch_one(&app.db)
        .await
        .expect("sticker template should be synced from disk by spawn_app");

    let response = app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/api/templates/{sticker_key}"))
                .body(Body::empty())
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

    assert_eq!(json["data"]["key"], sticker_key);
    assert_eq!(json["data"]["name"], "sticker");
    assert_eq!(json["data"]["type"], "label");
    assert_eq!(json["data"]["data"]["title"], "USB-C Cable");
    assert_eq!(json["data"]["data"]["reference"], "SKU-00042");
}

#[tokio::test]
async fn get_template_unknown_key_returns_404() {
    let app = common::spawn_app().await;

    let response = app
        .router
        .oneshot(
            Request::builder()
                .uri("/api/templates/tpl_does_not_exist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["code"], "TEMPLATE_NOT_FOUND");
}
