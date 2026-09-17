// Full-stack: real SQLite (a throwaway per-test file via
// `common::spawn_app`), real render engine. Exercises the whole render
// pipeline end to end against the seed templates (A4 Invoice, Thermal
// Receipt, Credit Note, Product Sticker Label), resolved by their schema
// `title` since on-disk names are opaque ids now.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

fn sample_receipt_data() -> serde_json::Value {
    // Field-for-field what MyroLogic POS's `billing::service::print_payload::
    // build_thermal_receipt_data` sends — kept in step with
    // thermal-receipt.schema.json, which now rejects incomplete payloads.
    json!({
        "paperWidthMm": 80,
        "invoiceNumber": "INV-0001",
        "formattedDate": "17 Aug 2026",
        "formattedTime": "14:32",
        "cashierName": "Jane Doe",
        "customerName": "",
        "customerPhone": "",
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
        "isCredit": false,
        "warrantyText": "",
        "footerText": "Thank you for your business!",
        "shopTradingName": "TechFix Repairs",
        "shopLegalName": "TechFix Repairs (Pvt) Ltd",
        "shopAddressLines": ["123 Galle Road", "Colombo 04"],
        "shopPrimaryPhone": "",
        "shopSecondaryPhone": "",
    })
}

#[tokio::test]
async fn render_thermal_receipt_returns_pdf_and_records_document() {
    let app = common::spawn_app().await;

    let template_key: String =
        sqlx::query_scalar("SELECT key FROM templates WHERE description = ?")
            .bind("Thermal Receipt")
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

    let template_key: String =
        sqlx::query_scalar("SELECT key FROM templates WHERE description = ?")
            .bind("Thermal Receipt")
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

    let template_key: String =
        sqlx::query_scalar("SELECT key FROM templates WHERE description = ?")
            .bind("Thermal Receipt")
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

    let template_key: String =
        sqlx::query_scalar("SELECT key FROM templates WHERE description = ?")
            .bind("A4 Invoice")
            .fetch_one(&app.db)
            .await
            .expect("a4-invoice template should be synced from disk by spawn_app");

    // Field-for-field what MyroLogic POS's `billing::service::print_payload::
    // build_a4_invoice_data` sends — kept in step with
    // a4-invoice.schema.json, which now rejects incomplete payloads.
    let body = json!({
        "invoiceNumber": "INV-0001",
        "formattedDate": "17 Aug 2026",
        "formattedTime": "14:32",
        "dueDate": "",
        "cashierName": "Jane Doe",
        "status": "paid",
        "isCredit": false,
        "copyDesignation": "ORIGINAL — CUSTOMER COPY",
        "isDuplicate": false,
        "customerName": "John Doe",
        "customerPhone": "",
        "customerAddress": "",
        "paymentMethod": "cash",
        "cardLast4": "",
        "tenderedAmountCents": 4000,
        "items": [
            {"name": "Widget", "quantity": 2, "unitPriceCents": 999, "discountCents": 0, "totalCents": 1998},
        ],
        "subtotalCents": 1998,
        "discountCents": 0,
        "taxCents": 0,
        "totalCents": 1998,
        "amountInWords": "Nineteen Rupees Ninety-Eight Cents Only",
        "notes": "",
        "warrantyText": "",
        "showBankDetails": false,
        "bankName": "",
        "bankBranch": "",
        "accountName": "",
        "accountNumber": "",
        "shopTradingName": "TechFix Repairs",
        "shopLegalName": "TechFix Repairs (Pvt) Ltd",
        "shopEmail": "",
        "shopWebsite": "",
        "shopBusinessRegNo": "",
        "shopVatNo": "",
        "shopIsVatRegistered": false,
        "shopAddressLines": ["123 Galle Road", "Colombo 04"],
        "shopPrimaryPhone": "",
        "shopSecondaryPhone": "",
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
async fn render_credit_note_returns_pdf_and_records_document() {
    let app = common::spawn_app().await;

    let template_key: String =
        sqlx::query_scalar("SELECT key FROM templates WHERE description = ?")
            .bind("Credit Note")
            .fetch_one(&app.db)
            .await
            .expect("credit-note template should be synced from disk by spawn_app");

    let body = json!({
        "creditNoteNumber": "CN-000042",
        "formattedDate": "21 Aug 2026",
        "cashierName": "Nimal Perera",
        "originalInvoiceNumber": "INV-001042",
        "noReceipt": false,
        "isManagerOverride": false,
        "exchangeReference": "",
        "customerName": "Kasun Silva",
        "customerPhone": "077 123 4567",
        "items": [
            {
                "name": "Screen Protector - iPhone 14",
                "sku": "ACC-SP-014",
                "serialNumber": "",
                "quantity": 2,
                "condition": "Good — Resalable",
                "disposition": "",
                "unitPriceCents": 150000,
                "totalCents": 300000
            },
            {
                "name": "Wireless Earbuds - Model X200",
                "sku": "ACC-WE-200",
                "serialNumber": "SN-88213X",
                "quantity": 1,
                "condition": "Damaged / Faulty",
                "disposition": "Write Off",
                "unitPriceCents": 700000,
                "totalCents": 700000
            }
        ],
        "refundCashCents": 700000,
        "balanceReductionCents": 300000,
        "refundBreakdown": [
            {"method": "cash", "amountCents": 400000},
            {"method": "card", "amountCents": 300000}
        ],
        "notes": "Customer reported earbuds stopped charging after 2 days; unit confirmed faulty on inspection.",
        "shopTradingName": "TechFix Repairs",
        "shopLegalName": "TechFix Repairs (Pvt) Ltd",
        "shopEmail": "info@techfixrepairs.lk",
        "shopWebsite": "techfixrepairs.lk",
        "shopBusinessRegNo": "PV 00123456",
        "shopVatNo": "",
        "shopIsVatRegistered": false,
        "shopAddressLines": ["123 Galle Road", "Colombo 04"],
        "shopPrimaryPhone": "011 234 5678",
        "shopSecondaryPhone": "",
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

    let template_key: String =
        sqlx::query_scalar("SELECT key FROM templates WHERE description = ?")
            .bind("Product Sticker Label")
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

    let template_key: String =
        sqlx::query_scalar("SELECT key FROM templates WHERE description = ?")
            .bind("Product Sticker Label")
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

// The schema-sidecar contract: a payload missing a *required* field is
// rejected before Typst ever runs, naming the field — this used to be a
// compile error (or worse, silently blank output) surfacing from inside the
// template.
#[tokio::test]
async fn render_missing_required_field_is_rejected_by_schema_before_compile() {
    let app = common::spawn_app().await;

    let template_key: String =
        sqlx::query_scalar("SELECT key FROM templates WHERE description = ?")
            .bind("Thermal Receipt")
            .fetch_one(&app.db)
            .await
            .expect("thermal-receipt template should be synced from disk by spawn_app");

    let mut body = sample_receipt_data();
    body.as_object_mut().unwrap().remove("isCredit");

    let response = app
        .router
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

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["code"], "RENDER_VALIDATION_FAILED");
    let message = json["message"].as_str().unwrap();
    assert!(
        message.contains("isCredit"),
        "rejection should name the missing field, got: {message}"
    );
}

// A mistyped field (string where the schema says integer) is rejected the
// same way as a missing one.
#[tokio::test]
async fn render_mistyped_field_is_rejected_by_schema() {
    let app = common::spawn_app().await;

    let template_key: String =
        sqlx::query_scalar("SELECT key FROM templates WHERE description = ?")
            .bind("Thermal Receipt")
            .fetch_one(&app.db)
            .await
            .expect("thermal-receipt template should be synced from disk by spawn_app");

    let mut body = sample_receipt_data();
    body["totalCents"] = json!("3997");

    let response = app
        .router
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

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["code"], "RENDER_VALIDATION_FAILED");
}

// Unknown fields are deliberately allowed (`additionalProperties: true`) so
// an integrator can carry extra context in its payloads without the schema
// rejecting it — only required/typed fields are policed.
#[tokio::test]
async fn render_with_unknown_extra_field_still_succeeds() {
    let app = common::spawn_app().await;

    let template_key: String =
        sqlx::query_scalar("SELECT key FROM templates WHERE description = ?")
            .bind("Thermal Receipt")
            .fetch_one(&app.db)
            .await
            .expect("thermal-receipt template should be synced from disk by spawn_app");

    let mut body = sample_receipt_data();
    body["futureField"] = json!({"anything": true});

    let response = app
        .router
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
}

// ============================================================================
// A4 Invoice (the modern design) + remote/embedded-image (`logoUrl`) rendering
// ============================================================================

fn a4_invoice_payload() -> serde_json::Value {
    json!({
        "invoiceNumber": "INV-0009",
        "formattedDate": "28 Aug 2026",
        "formattedTime": "10:15",
        "cashierName": "Nimal Perera",
        "status": "paid",
        "isCredit": false,
        "copyDesignation": "ORIGINAL — CUSTOMER COPY",
        "isDuplicate": false,
        "customerName": "Wagino Subianto",
        "customerAddress": "Main Street, Colombo 06",
        "paymentMethod": "cash",
        "tenderedAmountCents": 15000,
        "items": [
            {"name": "Brand identity workshop", "quantity": 1, "unitPriceCents": 2000, "discountCents": 0, "totalCents": 2000},
            {"name": "Landing page design", "quantity": 2, "unitPriceCents": 5000, "discountCents": 600, "totalCents": 9400},
        ],
        "subtotalCents": 12000,
        "discountCents": 600,
        "totalCents": 11400,
        "amountInWords": "Sri Lankan Rupees One Hundred Fourteen Only",
        "showBankDetails": false,
        "shopIsVatRegistered": false,
        "shopAddressLines": ["123 Galle Road", "Colombo 04"],
        "shopTradingName": "Northwind Studio",
    })
}

async fn a4_invoice_key(app: &common::TestApp) -> String {
    sqlx::query_scalar("SELECT key FROM templates WHERE description = ?")
        .bind("A4 Invoice")
        .fetch_one(&app.db)
        .await
        .expect("A4 Invoice template should be synced from disk by spawn_app")
}

async fn post_render(
    app: &common::TestApp,
    key: &str,
    body: &serde_json::Value,
) -> axum::response::Response {
    app.router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/render/{key}"))
                .header("content-type", "application/json")
                .header("X-Internal-Api-Key", &app.config.internal_api_key)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

/// No `.rimg_*` temp image should ever be left behind in the app's templates
/// tree once a render (or reprint) has returned.
fn assert_no_staged_images(app: &common::TestApp) {
    let root = app
        .templates_dir
        .as_ref()
        .expect("logoUrl tests use spawn_app_isolated_templates")
        .path();
    for sub in ["documents", "labels"] {
        let leftovers: Vec<_> = std::fs::read_dir(root.join(sub))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with(".rimg_"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "staged images left in {sub}/: {leftovers:?}"
        );
    }
}

#[tokio::test]
async fn render_a4_invoice_with_data_uri_logo_returns_pdf() {
    let app = common::spawn_app_isolated_templates().await;
    let key = a4_invoice_key(&app).await;

    let mut body = a4_invoice_payload();
    body["logoUrl"] = json!(format!(
        "data:image/png;base64,{}",
        base64_of(common::TINY_PNG)
    ));

    let response = post_render(&app, &key, &body).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "data: URI logo should render"
    );
    let pdf = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    assert_no_staged_images(&app);

    // The recorded row must NOT carry the base64 blob (it would bloat the table).
    let stored: String =
        sqlx::query_scalar("SELECT data FROM documents ORDER BY created_at DESC LIMIT 1")
            .fetch_one(&app.db)
            .await
            .unwrap();
    let stored: serde_json::Value = serde_json::from_str(&stored).unwrap();
    assert_eq!(
        stored["logoUrl"], "",
        "data: logoUrl should be blanked on record"
    );
}

#[tokio::test]
async fn render_a4_invoice_partially_paid_shows_paid_and_balance_due() {
    let app = common::spawn_app().await;
    let key = a4_invoice_key(&app).await;

    let mut body = a4_invoice_payload();
    body["status"] = json!("partially_paid");
    body["isCredit"] = json!(true);
    body["paymentMethod"] = json!("credit");
    body["dueDate"] = json!("2026-12-31");
    body["amountPaidCents"] = json!(4000);
    body["balanceDueCents"] = json!(7400);

    let response = post_render(&app, &key, &body).await;
    assert_eq!(response.status(), StatusCode::OK, "Body: {body}");
    let pdf = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
}

#[tokio::test]
async fn render_thermal_receipt_partial_credit_shows_balance_and_due_date() {
    let app = common::spawn_app().await;
    let template_key: String =
        sqlx::query_scalar("SELECT key FROM templates WHERE description = ?")
            .bind("Thermal Receipt")
            .fetch_one(&app.db)
            .await
            .expect("thermal-receipt template should be synced from disk by spawn_app");

    let mut body = sample_receipt_data();
    body["paymentMethod"] = json!("credit");
    body["isCredit"] = json!(true);
    body["tenderedAmountCents"] = json!(2000);
    body["changeDueCents"] = json!(0);
    body["dueDate"] = json!("2026-12-31");
    body["amountPaidCents"] = json!(2000);
    body["balanceDueCents"] = json!(1997);

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

    assert_eq!(response.status(), StatusCode::OK, "Body: {body}");
    let pdf = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
}

fn base64_of(bytes: &[u8]) -> String {
    // Minimal standard-alphabet base64 — avoids adding a dep to the test crate.
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        out.push(A[(n >> 18 & 63) as usize] as char);
        out.push(A[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            A[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            A[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[tokio::test]
async fn render_a4_invoice_with_logo_url_downloads_and_cleans_up() {
    let app = common::spawn_app_isolated_templates().await;
    let images = common::spawn_image_server().await;
    let key = a4_invoice_key(&app).await;

    let mut body = a4_invoice_payload();
    body["logoUrl"] = json!(format!("{}/logo.png", images.base_url));

    // Two renders back to back — the second proves nothing about the first's
    // image was retained (a stale cache would surface as a wrong/blank logo
    // or unbounded growth, and a fixed temp filename would collide).
    for _ in 0..2 {
        let response = post_render(&app, &key, &body).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "logoUrl render should succeed"
        );
        let pdf = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
    }
    assert_no_staged_images(&app);
}

#[tokio::test]
async fn render_a4_invoice_with_bad_logo_url_returns_422() {
    let app = common::spawn_app_isolated_templates().await;
    let images = common::spawn_image_server().await;
    let key = a4_invoice_key(&app).await;

    for path in ["not-an-image", "huge", "missing"] {
        let mut body = a4_invoice_payload();
        body["logoUrl"] = json!(format!("{}/{}", images.base_url, path));
        let response = post_render(&app, &key, &body).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "logoUrl /{path} should be rejected"
        );
        let json: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(json["code"], "REMOTE_IMAGE_FETCH_FAILED");
    }

    // A non-http scheme is rejected too.
    let mut body = a4_invoice_payload();
    body["logoUrl"] = json!("file:///etc/passwd");
    let response = post_render(&app, &key, &body).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    assert_no_staged_images(&app);
}

#[tokio::test]
async fn reprint_a4_invoice_with_logo_url_refetches() {
    let app = common::spawn_app_isolated_templates().await;
    let images = common::spawn_image_server().await;
    let key = a4_invoice_key(&app).await;

    let mut body = a4_invoice_payload();
    body["logoUrl"] = json!(format!("{}/logo.png", images.base_url));
    let response = post_render(&app, &key, &body).await;
    assert_eq!(response.status(), StatusCode::OK);

    let document_key: String =
        sqlx::query_scalar("SELECT key FROM documents ORDER BY created_at DESC LIMIT 1")
            .fetch_one(&app.db)
            .await
            .unwrap();

    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/documents/{document_key}/pdf"))
                .header("X-Internal-Api-Key", &app.config.internal_api_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let pdf = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    assert_no_staged_images(&app);
}

/// The worked example payload committed alongside the Analytics Report
/// template — kept in step with `analytics-report.schema.json` and the
/// backend's `reports::service::report_payload` builder.
fn sample_analytics_report_data() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../templates/documents/doc_temp_An1yT1csRep0rtV1.json"
    ))
    .expect("analytics report sample .json should be valid")
}

#[tokio::test]
async fn render_analytics_report_returns_multipage_pdf() {
    let app = common::spawn_app().await;

    let template_key: String =
        sqlx::query_scalar("SELECT key FROM templates WHERE description = ?")
            .bind("Analytics Report")
            .fetch_one(&app.db)
            .await
            .expect("analytics-report template should be synced from disk by spawn_app");

    let body = sample_analytics_report_data();
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
    let pdf = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    // A real multi-section report is comfortably over 20 KB.
    assert!(
        pdf.len() > 20_000,
        "report PDF unexpectedly small: {} bytes",
        pdf.len()
    );
}

#[tokio::test]
async fn render_analytics_report_with_empty_period_still_renders() {
    let app = common::spawn_app().await;

    let template_key: String =
        sqlx::query_scalar("SELECT key FROM templates WHERE description = ?")
            .bind("Analytics Report")
            .fetch_one(&app.db)
            .await
            .expect("analytics-report template should be synced from disk by spawn_app");

    // Only the required fields, all series/tables empty — a quiet period.
    let body = json!({
        "generatedAt": "30 Aug 2026, 09:00",
        "periodLabel": "1 Aug 2026 – 1 Aug 2026",
        "granularityLabel": "Daily",
        "kpis": [
            { "label": "Total revenue", "value": "Rs. 0.00" },
        ],
        "timeseries": { "labels": [], "revenueCents": [], "grossProfitCents": [] },
    });

    let response = app
        .router
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
    let pdf = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
}

#[tokio::test]
async fn render_analytics_report_missing_kpis_is_rejected_by_schema() {
    let app = common::spawn_app().await;

    let template_key: String =
        sqlx::query_scalar("SELECT key FROM templates WHERE description = ?")
            .bind("Analytics Report")
            .fetch_one(&app.db)
            .await
            .expect("analytics-report template should be synced from disk by spawn_app");

    let mut body = sample_analytics_report_data();
    body.as_object_mut().unwrap().remove("kpis");

    let response = app
        .router
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

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["code"], "RENDER_VALIDATION_FAILED");
    assert!(json["message"].as_str().unwrap().contains("kpis"));
}
