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
    assert_eq!(
        names,
        vec!["a4-invoice", "credit-note", "sticker", "thermal-receipt"]
    );
    assert!(templates.iter().all(|t| t["isActive"] == true));

    let invoice_tpl = templates
        .iter()
        .find(|t| t["name"] == "a4-invoice")
        .unwrap();
    assert_eq!(invoice_tpl["type"], "document");
    assert_eq!(invoice_tpl["data"]["invoiceNumber"], "INV-000123");
    assert!(invoice_tpl["data"]["items"].is_array());
    assert_eq!(invoice_tpl["data"]["totalCents"], 1100000);
    // The machine-readable input contract, parsed from the `.schema.json`
    // sidecar and exposed alongside the sample data.
    let required = invoice_tpl["dataSchema"]["required"].as_array().unwrap();
    assert!(required.iter().any(|r| r == "invoiceNumber"));
    assert!(required.iter().any(|r| r == "totalCents"));

    let credit_note_tpl = templates
        .iter()
        .find(|t| t["name"] == "credit-note")
        .unwrap();
    assert_eq!(credit_note_tpl["type"], "document");
    assert_eq!(credit_note_tpl["data"]["creditNoteNumber"], "CN-000042");
    assert!(credit_note_tpl["data"]["items"].is_array());
    assert_eq!(credit_note_tpl["data"]["refundCashCents"], 700000);

    let receipt_tpl = templates
        .iter()
        .find(|t| t["name"] == "thermal-receipt")
        .unwrap();
    assert_eq!(receipt_tpl["type"], "document");
    assert_eq!(receipt_tpl["data"]["invoiceNumber"], "INV-000123");
    assert!(receipt_tpl["data"]["items"].is_array());
    assert_eq!(receipt_tpl["data"]["totalCents"], 1100000);
    assert_eq!(receipt_tpl["data"]["paperWidthMm"], 80);

    let sticker_tpl = templates.iter().find(|t| t["name"] == "sticker").unwrap();
    assert_eq!(sticker_tpl["type"], "label");
    assert_eq!(sticker_tpl["data"]["title"], "USB-C Cable");
    assert_eq!(sticker_tpl["data"]["reference"], "SKU-00042");
    let required = sticker_tpl["dataSchema"]["required"].as_array().unwrap();
    assert!(required.iter().any(|r| r == "title"));
    assert!(required.iter().any(|r| r == "reference"));
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
    assert_eq!(templates.len(), 3);
    let names: Vec<&str> = templates
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["a4-invoice", "credit-note", "thermal-receipt"]);
    assert!(templates.iter().all(|t| t["type"] == "document"));

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
    // Single-template fetch exposes the schema contract too — this is what
    // an integration client reads to learn how to feed a render.
    let required = json["data"]["dataSchema"]["required"].as_array().unwrap();
    assert_eq!(required.len(), 2);
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

// ============================================================================
// Sync
// ============================================================================

fn post_sync_request() -> axum::http::Request<axum::body::Body> {
    Request::builder()
        .method("POST")
        .uri("/api/templates/sync")
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn sync_templates_requires_internal_api_key() {
    let app = common::spawn_app().await;

    let response = app.router.oneshot(post_sync_request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// A row whose `.typ` no longer exists on disk gets deactivated (never
// deleted) by the sync pass; everything still on disk stays active.
#[tokio::test]
async fn sync_deactivates_templates_removed_from_disk() {
    let app = common::spawn_app().await;

    sqlx::query(
        "INSERT INTO templates (key, name, type, description, data_schema, sample_data, is_active, created_at, updated_at)
         VALUES ('tpl_ghost', 'ghost-template', 'document', '', NULL, NULL, 1, datetime('now'), datetime('now'))",
    )
    .execute(&app.db)
    .await
    .expect("seed a stale template row");

    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/templates/sync")
                .header("X-Internal-Api-Key", &app.config.internal_api_key)
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
    assert_eq!(json["data"]["syncedCount"], 4);
    assert_eq!(json["data"]["deactivatedCount"], 1);
    let names: Vec<&str> = json["data"]["templates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        vec!["a4-invoice", "credit-note", "sticker", "thermal-receipt"]
    );

    // The ghost is deactivated but its row survives; real templates stay active.
    let ghost_active: bool =
        sqlx::query_scalar("SELECT is_active FROM templates WHERE key = 'tpl_ghost'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(!ghost_active);
    let active_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM templates WHERE is_active = 1")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(active_count, 4);
}

// The point of the sync endpoint: a `.typ` dropped into the templates dir
// becomes renderable without restarting the process, and removing it again
// deactivates it.
#[tokio::test]
async fn sync_picks_up_new_template_and_renders_it_without_restart() {
    let templates_root = tempfile::tempdir().expect("create temp templates root");
    copy_dir_recursive(std::path::Path::new("templates"), templates_root.path())
        .expect("copy seed templates into temp dir");

    let app = common::spawn_app_with_templates_dir(
        templates_root.path().to_str().expect("utf-8 temp path"),
    )
    .await;

    std::fs::write(
        templates_root.path().join("documents/hot-add.typ"),
        "= Hot Add\n\nAdded while the server was running.\n",
    )
    .expect("write new template file");

    let response = app
        .router
        .clone()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/templates/sync")
                .header("X-Internal-Api-Key", &app.config.internal_api_key)
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
    assert_eq!(json["data"]["syncedCount"], 5);

    let hot_key: String = sqlx::query_scalar("SELECT key FROM templates WHERE name = 'hot-add'")
        .fetch_one(&app.db)
        .await
        .expect("hot-add should be synced");

    // No schema sidecar → `{}` renders fine: validation only applies when a
    // template ships one.
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/render/{hot_key}"))
                .header("content-type", "application/json")
                .header("X-Internal-Api-Key", &app.config.internal_api_key)
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let pdf_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(pdf_bytes.starts_with(b"%PDF-"));

    // Removing the file and re-syncing deactivates it — future renders 404.
    std::fs::remove_file(templates_root.path().join("documents/hot-add.typ"))
        .expect("remove new template file");
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/templates/sync")
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
                .method("POST")
                .uri(format!("/api/render/{hot_key}"))
                .header("content-type", "application/json")
                .header("X-Internal-Api-Key", &app.config.internal_api_key)
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}
