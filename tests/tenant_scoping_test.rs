// Tenant isolation for the multi-tenant cloud deployment (one document-server
// instance shared by every tenant of the cloud POS backend, distinguished by
// the `X-Tenant-Key` header on `InternalCaller` — see
// `core::middleware::auth::TenantKey` and CLAUDE.md's "Tenant scoping"
// section). Full-stack, real SQLite (throwaway per-test file via
// `common::spawn_app`).

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

const TENANT_A: &str = "tnt_a1111111111111";
const TENANT_B: &str = "tnt_b2222222222222";

fn put_data(
    template_name: &str,
    data_key: &str,
    api_key: &str,
    tenant: Option<&str>,
    body: Value,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method("PUT")
        .uri(format!("/api/template-data/{template_name}/{data_key}"))
        .header("content-type", "application/json")
        .header("X-Internal-Api-Key", api_key);
    if let Some(t) = tenant {
        builder = builder.header("X-Tenant-Key", t);
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

fn get_data(
    template_name: &str,
    data_key: &str,
    api_key: &str,
    tenant: Option<&str>,
) -> Request<Body> {
    let mut builder = Request::builder()
        .uri(format!("/api/template-data/{template_name}/{data_key}"))
        .header("X-Internal-Api-Key", api_key);
    if let Some(t) = tenant {
        builder = builder.header("X-Tenant-Key", t);
    }
    builder.body(Body::empty()).unwrap()
}

fn delete_data(
    template_name: &str,
    data_key: &str,
    api_key: &str,
    tenant: Option<&str>,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method("DELETE")
        .uri(format!("/api/template-data/{template_name}/{data_key}"))
        .header("X-Internal-Api-Key", api_key);
    if let Some(t) = tenant {
        builder = builder.header("X-Tenant-Key", t);
    }
    builder.body(Body::empty()).unwrap()
}

async fn get_json(response: axum::response::Response) -> Value {
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap()
}

/// The Thermal Receipt seed template's generated name / key, resolved via
/// its human label — same helper `template_data_test.rs` uses.
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

fn list_documents(api_key: &str, tenant: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .uri("/api/documents")
        .header("X-Internal-Api-Key", api_key);
    if let Some(t) = tenant {
        builder = builder.header("X-Tenant-Key", t);
    }
    builder.body(Body::empty()).unwrap()
}

async fn render_receipt(
    app: &common::TestApp,
    template_key: &str,
    tenant: Option<&str>,
    body: &Value,
) -> axum::response::Response {
    let mut builder = Request::builder()
        .method("POST")
        .uri(format!("/api/render/{template_key}"))
        .header("content-type", "application/json")
        .header("X-Internal-Api-Key", &app.config.internal_api_key);
    if let Some(t) = tenant {
        builder = builder.header("X-Tenant-Key", t);
    }
    app.router
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

// ----------------------------------------------------------------------------
// template_data
// ----------------------------------------------------------------------------

#[tokio::test]
async fn two_tenants_storing_under_the_identical_key_never_see_each_others_blob() {
    let app = common::spawn_app().await;
    let tr = tr_name(&app).await;

    let response = app
        .router
        .clone()
        .oneshot(put_data(
            &tr,
            "shop",
            &app.config.internal_api_key,
            Some(TENANT_A),
            json!({"shopTradingName": "A Shop"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .router
        .clone()
        .oneshot(put_data(
            &tr,
            "shop",
            &app.config.internal_api_key,
            Some(TENANT_B),
            json!({"shopTradingName": "B Shop"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .router
        .clone()
        .oneshot(get_data(
            &tr,
            "shop",
            &app.config.internal_api_key,
            Some(TENANT_A),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        get_json(response).await["data"]["data"]["shopTradingName"],
        "A Shop"
    );

    let response = app
        .router
        .clone()
        .oneshot(get_data(
            &tr,
            "shop",
            &app.config.internal_api_key,
            Some(TENANT_B),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        get_json(response).await["data"]["data"]["shopTradingName"],
        "B Shop"
    );

    // The single-shop bucket (no header at all) never stored anything here.
    let response = app
        .router
        .clone()
        .oneshot(get_data(&tr, "shop", &app.config.internal_api_key, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn omitting_the_header_is_a_third_bucket_isolated_from_every_tenant() {
    let app = common::spawn_app().await;
    let tr = tr_name(&app).await;

    // Single-shop / desktop: never sends X-Tenant-Key at all.
    let response = app
        .router
        .clone()
        .oneshot(put_data(
            &tr,
            "shop",
            &app.config.internal_api_key,
            None,
            json!({"shopTradingName": "Single Shop"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .router
        .clone()
        .oneshot(put_data(
            &tr,
            "shop",
            &app.config.internal_api_key,
            Some(TENANT_A),
            json!({"shopTradingName": "A Shop"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .router
        .clone()
        .oneshot(get_data(&tr, "shop", &app.config.internal_api_key, None))
        .await
        .unwrap();
    assert_eq!(
        get_json(response).await["data"]["data"]["shopTradingName"],
        "Single Shop",
        "single-shop callers keep reading their own bucket, exactly as before tenants existed"
    );

    let response = app
        .router
        .clone()
        .oneshot(get_data(
            &tr,
            "shop",
            &app.config.internal_api_key,
            Some(TENANT_A),
        ))
        .await
        .unwrap();
    assert_eq!(
        get_json(response).await["data"]["data"]["shopTradingName"],
        "A Shop"
    );

    // A tenant that never stored anything gets a clean 404, not someone else's blob.
    let response = app
        .router
        .clone()
        .oneshot(get_data(
            &tr,
            "shop",
            &app.config.internal_api_key,
            Some(TENANT_B),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_invalid_tenant_key_is_rejected() {
    let app = common::spawn_app().await;
    let tr = tr_name(&app).await;

    for bad in ["has a space", "has/a/slash", &"x".repeat(129)] {
        let response = app
            .router
            .clone()
            .oneshot(put_data(
                &tr,
                "shop",
                &app.config.internal_api_key,
                Some(bad),
                json!({}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{bad:?}");
        let json = get_json(response).await;
        assert_eq!(json["code"], "TENANT_KEY_INVALID");
    }

    // An unauthenticated caller doesn't even get a tenant-format error — the
    // API key check runs first, so a malformed tenant header can't be used
    // to distinguish "wrong key" from "wrong key, but nice tenant id" either.
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/template-data/{tr}"))
                .header("X-Tenant-Key", "has a space")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ----------------------------------------------------------------------------
// render / documents
// ----------------------------------------------------------------------------

#[tokio::test]
async fn a_render_records_the_calling_tenant_and_stored_blobs_stay_scoped_to_it() {
    let app = common::spawn_app().await;
    let tr = tr_name(&app).await;
    let template_key = tr_key(&app).await;

    // Only tenant A stores the blob the payload is missing.
    let response = app
        .router
        .clone()
        .oneshot(put_data(
            &tr,
            "defaults",
            &app.config.internal_api_key,
            Some(TENANT_A),
            json!({"isCredit": false}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = receipt_payload_missing(&["isCredit"]);

    // Tenant A's render is completed by its own stored blob.
    let response = render_receipt(&app, &template_key, Some(TENANT_A), &body).await;
    assert_eq!(response.status(), StatusCode::OK);
    let pdf_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(pdf_bytes.starts_with(b"%PDF-"));

    // Tenant B has no such blob — the same incomplete payload is rejected,
    // proving tenant A's blob did not leak into tenant B's render.
    let response = render_receipt(&app, &template_key, Some(TENANT_B), &body).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // The recorded `documents` row is tagged with tenant A, not left blank
    // or defaulted to the single-shop bucket.
    let tenant_key: String =
        sqlx::query_scalar("SELECT tenant_key FROM documents ORDER BY created_at DESC LIMIT 1")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(tenant_key, TENANT_A);
}

#[tokio::test]
async fn listing_documents_is_scoped_to_the_callers_tenant() {
    let app = common::spawn_app().await;
    let template_key = tr_key(&app).await;
    // A schema-complete payload needs no stored `template_data` default, so
    // this proves list scoping on its own, independent of the render/merge
    // scoping the other tests here already cover.
    let body = receipt_payload_missing(&[]);

    // One document each for tenant A, tenant B, and the single-shop (no
    // header) bucket.
    for tenant in [Some(TENANT_A), Some(TENANT_B), None] {
        let response = render_receipt(&app, &template_key, tenant, &body).await;
        assert_eq!(response.status(), StatusCode::OK, "{tenant:?}");
    }

    let response = app
        .router
        .clone()
        .oneshot(list_documents(&app.config.internal_api_key, Some(TENANT_A)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = get_json(response).await;
    assert_eq!(
        json["data"]["documents"].as_array().unwrap().len(),
        1,
        "tenant A must see only its own document, not B's or the single-shop bucket's"
    );

    let response = app
        .router
        .clone()
        .oneshot(list_documents(&app.config.internal_api_key, Some(TENANT_B)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = get_json(response).await;
    assert_eq!(json["data"]["documents"].as_array().unwrap().len(), 1);

    // No header at all: the single-shop bucket, unaffected by either tenant's
    // render — exactly as `documents_test.rs`'s list test already assumes
    // for the byte-identical no-tenant case.
    let response = app
        .router
        .clone()
        .oneshot(list_documents(&app.config.internal_api_key, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = get_json(response).await;
    assert_eq!(json["data"]["documents"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn reprint_merges_with_the_documents_own_tenant_not_the_reprint_callers_header() {
    let app = common::spawn_app().await;
    let tr = tr_name(&app).await;
    let template_key = tr_key(&app).await;

    let response = app
        .router
        .clone()
        .oneshot(put_data(
            &tr,
            "defaults",
            &app.config.internal_api_key,
            Some(TENANT_A),
            json!({"isCredit": false}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = receipt_payload_missing(&["isCredit"]);
    let response = render_receipt(&app, &template_key, Some(TENANT_A), &body).await;
    assert_eq!(response.status(), StatusCode::OK);

    let document_key: String =
        sqlx::query_scalar("SELECT key FROM documents ORDER BY created_at DESC LIMIT 1")
            .fetch_one(&app.db)
            .await
            .unwrap();

    // Reprinting carries NO tenant header at all (a single-shop caller, or a
    // caller that forgot one) and still succeeds — the merge uses the
    // document's own recorded tenant (A), not whatever the caller sent.
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
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "reprint must use the document's own tenant, not the caller's (absent) header"
    );

    // Same again, this time with tenant B's header on the reprint call —
    // still succeeds, still against tenant A's blob, never tenant B's.
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/documents/{document_key}/pdf"))
                .header("X-Internal-Api-Key", &app.config.internal_api_key)
                .header("X-Tenant-Key", TENANT_B)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "reprint must ignore the caller's tenant header entirely"
    );

    // Proof it really is reading tenant A's blob specifically: delete it
    // (as tenant A) and the exact same no-header reprint now fails, because
    // the recorded payload alone no longer satisfies the schema.
    let response = app
        .router
        .clone()
        .oneshot(delete_data(
            &tr,
            "defaults",
            &app.config.internal_api_key,
            Some(TENANT_A),
        ))
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
