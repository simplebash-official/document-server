// Full-stack: real SQLite (a throwaway per-test file via
// `common::spawn_app`), real render engine. Exercises the whole render
// pipeline end to end against the seed `templates/invoice.typ` template.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

fn sample_invoice_data() -> serde_json::Value {
    json!({
        "invoiceNumber": "INV-0001",
        "customerName": "Jane Doe",
        "items": [
            {"description": "Widget", "quantity": 2, "unitPrice": 9.99},
            {"description": "Gadget", "quantity": 1, "unitPrice": 19.99},
        ],
        "total": 39.97,
    })
}

#[tokio::test]
async fn render_invoice_returns_pdf_and_records_document() {
    let app = common::spawn_app().await;

    let template_key: String = sqlx::query_scalar("SELECT key FROM templates WHERE name = ?")
        .bind("invoice")
        .fetch_one(&app.db)
        .await
        .expect("invoice template should be synced from disk by spawn_app");

    let body = sample_invoice_data();
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
        "response body does not start with the PDF magic bytes"
    );

    let (file_size_bytes, data_text): (i64, String) =
        sqlx::query_as("SELECT file_size_bytes, data FROM documents WHERE template_key = ?")
            .bind(&template_key)
            .fetch_one(&app.db)
            .await
            .expect("render_template should have recorded a documents entry");

    assert_eq!(file_size_bytes, pdf_bytes.len() as i64);
    let recorded_data: serde_json::Value =
        serde_json::from_str(&data_text).expect("data column should be JSON");
    assert_eq!(recorded_data, body);
}

#[tokio::test]
async fn render_unknown_template_returns_404() {
    let app = common::spawn_app().await;

    let response = app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/render/tpl_does_not_exist")
                .header("content-type", "application/json")
                .body(Body::from(sample_invoice_data().to_string()))
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

#[tokio::test]
async fn render_invoice_with_missing_field_returns_422() {
    let app = common::spawn_app().await;

    let template_key: String = sqlx::query_scalar("SELECT key FROM templates WHERE name = ?")
        .bind("invoice")
        .fetch_one(&app.db)
        .await
        .expect("invoice template should be synced from disk by spawn_app");

    // Missing every field `invoice.typ` reads off `data` — fails at
    // `data.invoiceNumber` access during compilation.
    let response = app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/render/{template_key}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["code"], "RENDER_VALIDATION_FAILED");
}
