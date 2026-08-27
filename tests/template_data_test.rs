// Full-stack coverage for the template-data module (`/api/template-data`)
// and its render-time effect: stored blobs are deep-merged under render
// payloads (request wins) on both first render and reprint.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

fn put_data(template_name: &str, data_key: &str, api_key: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(format!("/api/template-data/{template_name}/{data_key}"))
        .header("content-type", "application/json")
        .header("X-Internal-Api-Key", api_key)
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn get_json(response: axum::response::Response) -> Value {
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap()
}

/// The Thermal Receipt seed template's generated name / key — resolved via
/// the human label in its schema `title` (the `templates.description`
/// column), since the on-disk name is now an opaque id.
async fn tr_name(app: &common::TestApp) -> String {
    sqlx::query_scalar("SELECT name FROM templates WHERE description = 'Thermal Receipt'")
        .fetch_one(&app.db)
        .await
        .expect("Thermal Receipt template should be synced from disk")
}

async fn tr_key(app: &common::TestApp) -> String {
    sqlx::query_scalar("SELECT key FROM templates WHERE description = 'Thermal Receipt'")
        .fetch_one(&app.db)
        .await
        .expect("Thermal Receipt template should be synced from disk")
}

// A receipt payload that is schema-valid EXCEPT for the three fields listed
// here as missing — the stored blob under test supplies exactly those.
fn receipt_payload_missing(missing: &[&str]) -> Value {
    let mut body = json!({
        "paperWidthMm": 80,
        "invoiceNumber": "INV-0001",
        "formattedDate": "17 Aug 2026",
        "formattedTime": "14:32",
        "cashierName": "Jane Doe",
        "items": [
            {"name": "Widget", "quantity": 2, "unitPriceCents": 999, "discountCents": 0, "totalCents": 1998},
        ],
        "subtotalCents": 1998,
        "discountCents": 0,
        "taxCents": 0,
        "totalCents": 1998,
        "paymentMethod": "cash",
        "tenderedAmountCents": 2000,
        "changeDueCents": 2,
        "isCredit": false,
        "shopTradingName": "TechFix Repairs",
        "shopLegalName": "TechFix Repairs (Pvt) Ltd",
        "shopAddressLines": ["123 Galle Road", "Colombo 04"],
    });
    let obj = body.as_object_mut().unwrap();
    for field in missing {
        obj.remove(*field);
    }
    body
}

async fn render_receipt(
    app: &common::TestApp,
    template_key: &str,
    body: &Value,
) -> axum::response::Response {
    app.router
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
        .unwrap()
}

#[tokio::test]
async fn template_data_routes_require_internal_api_key() {
    let app = common::spawn_app().await;
    let tr = tr_name(&app).await;

    // PUT without a key
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/api/template-data/{tr}/shop"))
        .header("content-type", "application/json")
        .body(Body::from(json!({}).to_string()))
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // GET list without a key
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/template-data/{tr}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn set_get_list_delete_roundtrip() {
    let app = common::spawn_app().await;
    let tr = tr_name(&app).await;

    let response = app
        .router
        .clone()
        .oneshot(put_data(
            &tr,
            "shop",
            &app.config.internal_api_key,
            json!({"shopTradingName": "Stored Shop"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = get_json(response).await;
    let key = json["data"]["key"].as_str().unwrap();
    assert!(key.starts_with("tdat_"));
    assert_eq!(json["data"]["templateName"], tr);
    assert_eq!(json["data"]["dataKey"], "shop");

    // GET one
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/template-data/{tr}/shop"))
                .header("X-Internal-Api-Key", &app.config.internal_api_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        get_json(response).await["data"]["data"]["shopTradingName"],
        "Stored Shop"
    );

    // PUT again with the same dataKey replaces, not duplicates
    let response = app
        .router
        .clone()
        .oneshot(put_data(
            &tr,
            "shop",
            &app.config.internal_api_key,
            json!({"shopTradingName": "Replaced Shop"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/template-data/{tr}"))
                .header("X-Internal-Api-Key", &app.config.internal_api_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json = get_json(response).await;
    assert_eq!(json["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        json["data"]["items"][0]["data"]["shopTradingName"],
        "Replaced Shop"
    );

    // DELETE returns the removed blob; a second delete 404s
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/template-data/{tr}/shop"))
                .header("X-Internal-Api-Key", &app.config.internal_api_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        get_json(response).await["data"]["data"]["shopTradingName"],
        "Replaced Shop"
    );

    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/template-data/{tr}/shop"))
                .header("X-Internal-Api-Key", &app.config.internal_api_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let json = get_json(response).await;
    assert_eq!(json["code"], "TEMPLATE_DATA_NOT_FOUND");
}

#[tokio::test]
async fn set_for_unknown_template_returns_404() {
    let app = common::spawn_app().await;

    let response = app
        .router
        .clone()
        .oneshot(put_data(
            "no-such-template",
            "shop",
            &app.config.internal_api_key,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let json = get_json(response).await;
    assert_eq!(json["code"], "TEMPLATE_NOT_FOUND");
}

// The reason the feature exists: a render whose payload is missing required
// fields succeeds when the stored blob supplies them — validation runs on
// the merged input.
#[tokio::test]
async fn stored_blob_supplies_required_fields_at_render_time() {
    let app = common::spawn_app().await;
    let tr = tr_name(&app).await;

    let response = app
        .router
        .clone()
        .oneshot(put_data(
            &tr,
            "defaults",
            &app.config.internal_api_key,
            json!({
                "isCredit": false,
                "shopTradingName": "Stored Shop",
                "shopLegalName": "Stored Shop (Pvt) Ltd"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let template_key = tr_key(&app).await;

    let body = receipt_payload_missing(&["isCredit", "shopTradingName"]);
    let response = render_receipt(&app, &template_key, &body).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "stored blob should fill the missing fields"
    );
    let pdf_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(pdf_bytes.starts_with(b"%PDF-"));
}

// The merge rule the billing flow depends on: what the request sends wins
// over any stored default. Proven by storing a value that would FAIL the
// schema for a field the request provides correctly.
#[tokio::test]
async fn request_payload_overrides_stored_blob() {
    let app = common::spawn_app().await;
    let tr = tr_name(&app).await;

    let response = app
        .router
        .clone()
        .oneshot(put_data(
            &tr,
            "bad-defaults",
            &app.config.internal_api_key,
            json!({"paymentMethod": "not-a-real-method"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let template_key = tr_key(&app).await;

    let body = receipt_payload_missing(&[]);
    let response = render_receipt(&app, &template_key, &body).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "the request's valid paymentMethod must win over the stored bad one"
    );

    // And the same stored bad value DOES reject a request that omits it —
    // proving the blob was merged (and overridden), not ignored.
    let mut body = receipt_payload_missing(&[]);
    body.as_object_mut().unwrap().remove("paymentMethod");
    let response = render_receipt(&app, &template_key, &body).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

// Reprints re-apply the merge against *current* stored state: a document
// rendered while a blob existed stops reprinting once that blob is gone
// (the recorded request payload alone no longer validates).
#[tokio::test]
async fn reprint_reapplies_current_stored_state() {
    let app = common::spawn_app().await;
    let tr = tr_name(&app).await;

    let response = app
        .router
        .clone()
        .oneshot(put_data(
            &tr,
            "defaults",
            &app.config.internal_api_key,
            json!({"isCredit": false}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let template_key = tr_key(&app).await;

    let body = receipt_payload_missing(&["isCredit"]);
    let response = render_receipt(&app, &template_key, &body).await;
    assert_eq!(response.status(), StatusCode::OK);

    let document_key: String =
        sqlx::query_scalar("SELECT key FROM documents ORDER BY created_at DESC LIMIT 1")
            .fetch_one(&app.db)
            .await
            .unwrap();

    // Reprint works while the blob still supplies `isCredit`.
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

    // Remove the blob → the recorded request alone no longer satisfies the
    // schema, and the reprint says so instead of silently rendering blanks.
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/template-data/{tr}/defaults"))
                .header("X-Internal-Api-Key", &app.config.internal_api_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .router
        .oneshot(
            Request::builder()
                .uri(format!("/api/documents/{document_key}/pdf"))
                .header("X-Internal-Api-Key", &app.config.internal_api_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
