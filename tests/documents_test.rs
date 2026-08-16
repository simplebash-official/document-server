// Full-stack, real SQLite (throwaway per-test file via `common::spawn_app`).
// Covers the real `GET /api/documents`, `GET /api/documents/{key}`, and
// `GET /api/documents/{key}/pdf` (reprint) read routes added in Phase 3 —
// `render_test.rs` already covers the write path (recording a document on
// a successful render), this file is scoped to documents-as-a-resource.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

/// Renders the seed `sticker` template once and returns the `documents.key`
/// it produced — the fixture every test in this file needs.
async fn render_a_sticker(app: &common::TestApp) -> String {
    let template_key: String = sqlx::query_scalar("SELECT key FROM templates WHERE name = ?")
        .bind("sticker")
        .fetch_one(&app.db)
        .await
        .expect("sticker template should be synced from disk by spawn_app");

    let body = json!({"title": "USB-C Cable", "reference": "SKU-00042"});
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/render/{template_key}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    sqlx::query_scalar("SELECT key FROM documents WHERE template_key = ?")
        .bind(&template_key)
        .fetch_one(&app.db)
        .await
        .expect("render should have recorded exactly one documents row")
}

#[tokio::test]
async fn list_documents_returns_every_recorded_document() {
    let app = common::spawn_app().await;
    let key = render_a_sticker(&app).await;

    let response = app
        .router
        .oneshot(
            Request::builder()
                .uri("/api/documents")
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

    let documents = json["data"]["documents"].as_array().unwrap();
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0]["key"], key);
    assert_eq!(documents[0]["data"]["reference"], "SKU-00042");
}

#[tokio::test]
async fn get_document_by_key_returns_that_document() {
    let app = common::spawn_app().await;
    let key = render_a_sticker(&app).await;

    let response = app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/api/documents/{key}"))
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

    assert_eq!(json["data"]["key"], key);
    assert_eq!(json["data"]["data"]["title"], "USB-C Cable");
}

#[tokio::test]
async fn get_document_unknown_key_returns_404() {
    let app = common::spawn_app().await;

    let response = app
        .router
        .oneshot(
            Request::builder()
                .uri("/api/documents/doc_does_not_exist")
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
    assert_eq!(json["code"], "DOCUMENT_NOT_FOUND");
}

#[tokio::test]
async fn reprint_document_returns_pdf() {
    let app = common::spawn_app().await;
    let key = render_a_sticker(&app).await;

    let response = app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/api/documents/{key}/pdf"))
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
        Some("application/pdf")
    );

    let pdf_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(
        pdf_bytes.starts_with(b"%PDF-"),
        "reprint response body does not start with the PDF magic bytes"
    );
}

#[tokio::test]
async fn reprint_unknown_document_returns_404() {
    let app = common::spawn_app().await;

    let response = app
        .router
        .oneshot(
            Request::builder()
                .uri("/api/documents/doc_does_not_exist/pdf")
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
    assert_eq!(json["code"], "DOCUMENT_NOT_FOUND");
}
