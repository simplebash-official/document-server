// Full-stack: real SQLite (a throwaway per-test file via
// `common::spawn_app`), real render engine. Exercises the whole render
// pipeline end to end against the seed `templates/documents/a4-invoice.typ`,
// `templates/documents/thermal-receipt.typ`, and `templates/labels/sticker.typ`
// templates.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

fn sample_receipt_data() -> serde_json::Value {
    json!({
        "paperWidthMm": 80,
        "invoiceNumber": "INV-0001",
        "formattedDate": "17 Aug 2026",
        "formattedTime": "14:32",
        "cashierName": "Jane Doe",
        "items": [
            {"name": "Widget", "quantity": 2, "unitPriceCents": 999, "discountCents": 0, "totalCents": 1998},
            {"name": "Gadget", "quantity": 1, "unitPriceCents": 1999, "discountCents": 0, "totalCents": 1999},
        ],
        "subtotalCents": 3997,
        "discountCents": 0,
        "taxCents": 0,
        "totalCents": 3997,
        "paymentMethod": "cash",
        "tenderedAmountCents": 4000,
        "changeDueCents": 3,
    })
}

#[tokio::test]
async fn render_thermal_receipt_returns_pdf_and_records_document() {
    let app = common::spawn_app().await;

    let template_key: String = sqlx::query_scalar("SELECT key FROM templates WHERE name = ?")
        .bind("thermal-receipt")
        .fetch_one(&app.db)
        .await
        .expect("thermal-receipt template should be synced from disk by spawn_app");

    let body = sample_receipt_data();
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/render/{template_key}"))
                .header("content-type", "application/json")
                .header("X-Internal-Api-Key", &app.config.internal_api_key)
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
async fn render_thermal_receipt_at_58mm_and_80mm_both_succeed() {
    let app = common::spawn_app().await;

    let template_key: String = sqlx::query_scalar("SELECT key FROM templates WHERE name = ?")
        .bind("thermal-receipt")
        .fetch_one(&app.db)
        .await
        .expect("thermal-receipt template should be synced from disk by spawn_app");

    for width in [58, 80] {
        let mut body = sample_receipt_data();
        body["paperWidthMm"] = json!(width);

        let response = app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/render/{template_key}"))
                    .header("content-type", "application/json")
                    .header("X-Internal-Api-Key", &app.config.internal_api_key)
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK, "paperWidthMm={width}");
        let pdf_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(pdf_bytes.starts_with(b"%PDF-"));
    }
}

#[tokio::test]
async fn render_without_internal_api_key_header_returns_401() {
    let app = common::spawn_app().await;

    let template_key: String = sqlx::query_scalar("SELECT key FROM templates WHERE name = ?")
        .bind("thermal-receipt")
        .fetch_one(&app.db)
        .await
        .expect("thermal-receipt template should be synced from disk by spawn_app");

    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/render/{template_key}"))
                .header("content-type", "application/json")
                .body(Body::from(sample_receipt_data().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["code"], "INTERNAL_API_KEY_INVALID");

    // Wrong key is rejected the same way as a missing one.
    let response = app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/render/{template_key}"))
                .header("content-type", "application/json")
                .header("X-Internal-Api-Key", "not-the-real-key")
                .body(Body::from(sample_receipt_data().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn render_a4_invoice_returns_pdf_and_records_document() {
    let app = common::spawn_app().await;

    let template_key: String = sqlx::query_scalar("SELECT key FROM templates WHERE name = ?")
        .bind("a4-invoice")
        .fetch_one(&app.db)
        .await
        .expect("a4-invoice template should be synced from disk by spawn_app");

    let body = json!({
        "invoiceNumber": "INV-0001",
        "formattedDate": "17 Aug 2026",
        "formattedTime": "14:32",
        "cashierName": "Jane Doe",
        "status": "paid",
        "isCredit": false,
        "customerName": "John Doe",
        "paymentMethod": "cash",
        "tenderedAmountCents": 4000,
        "items": [
            {"name": "Widget", "quantity": 2, "unitPriceCents": 999, "discountCents": 0, "totalCents": 1998},
        ],
        "subtotalCents": 1998,
        "discountCents": 0,
        "taxCents": 0,
        "totalCents": 1998,
        "amountInWords": "Nineteen Rupees Ninety-Eight Cents Only",
    });

    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/render/{template_key}"))
                .header("content-type", "application/json")
                .header("X-Internal-Api-Key", &app.config.internal_api_key)
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

    let file_size_bytes: i64 =
        sqlx::query_scalar("SELECT file_size_bytes FROM documents WHERE template_key = ?")
            .bind(&template_key)
            .fetch_one(&app.db)
            .await
            .expect("render_template should have recorded a documents entry");
    assert_eq!(file_size_bytes, pdf_bytes.len() as i64);
}

#[tokio::test]
async fn render_sticker_returns_pdf_and_records_document() {
    let app = common::spawn_app().await;

    let template_key: String = sqlx::query_scalar("SELECT key FROM templates WHERE name = ?")
        .bind("sticker")
        .fetch_one(&app.db)
        .await
        .expect("sticker template should be synced from disk by spawn_app");

    let body = json!({
        "title": "USB-C Cable",
        "reference": "SKU-00042",
    });
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/render/{template_key}"))
                .header("content-type", "application/json")
                .header("X-Internal-Api-Key", &app.config.internal_api_key)
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

    let file_size_bytes: i64 =
        sqlx::query_scalar("SELECT file_size_bytes FROM documents WHERE template_key = ?")
            .bind(&template_key)
            .fetch_one(&app.db)
            .await
            .expect("render_template should have recorded a documents entry");
    assert_eq!(file_size_bytes, pdf_bytes.len() as i64);
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
                .header("X-Internal-Api-Key", &app.config.internal_api_key)
                .body(Body::from(sample_receipt_data().to_string()))
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
async fn render_sticker_with_missing_field_returns_422() {
    let app = common::spawn_app().await;

    let template_key: String = sqlx::query_scalar("SELECT key FROM templates WHERE name = ?")
        .bind("sticker")
        .fetch_one(&app.db)
        .await
        .expect("sticker template should be synced from disk by spawn_app");

    // Missing every field `sticker.typ` reads off `data` — fails at
    // `data.title` access during compilation.
    let response = app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/render/{template_key}"))
                .header("content-type", "application/json")
                .header("X-Internal-Api-Key", &app.config.internal_api_key)
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
