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
    // Names are opaque generated ids now (`doc_temp_…` / `lbl_temp_…`); a
    // template is identified by its `description` (the schema `title`).
    let mut descriptions: Vec<&str> = templates
        .iter()
        .map(|t| t["description"].as_str().unwrap())
        .collect();
    descriptions.sort_unstable();
    assert_eq!(
        descriptions,
        vec![
            "A4 Invoice",
            "Credit Note",
            "Product Sticker Label",
            "Thermal Receipt",
        ]
    );
    assert!(templates.iter().all(|t| t["isActive"] == true));
    assert!(templates.iter().all(|t| {
        let name = t["name"].as_str().unwrap();
        name.starts_with("doc_temp_") || name.starts_with("lbl_temp_")
    }));

    let by_desc = |d: &str| -> serde_json::Value {
        templates
            .iter()
            .find(|t| t["description"] == d)
            .unwrap()
            .clone()
    };

    let invoice_tpl = by_desc("A4 Invoice");
    assert_eq!(invoice_tpl["type"], "document");
    assert_eq!(invoice_tpl["data"]["invoiceNumber"], "INV-000123");
    assert!(invoice_tpl["data"]["items"].is_array());
    assert_eq!(invoice_tpl["data"]["totalCents"], 1100000);
    // The machine-readable input contract, parsed from the `.schema.json`
    // sidecar and exposed alongside the sample data.
    let required = invoice_tpl["dataSchema"]["required"].as_array().unwrap();
    assert!(required.iter().any(|r| r == "invoiceNumber"));
    assert!(required.iter().any(|r| r == "totalCents"));

    let credit_note_tpl = by_desc("Credit Note");
    assert_eq!(credit_note_tpl["type"], "document");
    assert_eq!(credit_note_tpl["data"]["creditNoteNumber"], "CN-000042");
    assert!(credit_note_tpl["data"]["items"].is_array());
    assert_eq!(credit_note_tpl["data"]["refundCashCents"], 700000);

    let receipt_tpl = by_desc("Thermal Receipt");
    assert_eq!(receipt_tpl["type"], "document");
    assert_eq!(receipt_tpl["data"]["invoiceNumber"], "INV-000123");
    assert!(receipt_tpl["data"]["items"].is_array());
    assert_eq!(receipt_tpl["data"]["totalCents"], 1100000);
    assert_eq!(receipt_tpl["data"]["paperWidthMm"], 80);

    let sticker_tpl = by_desc("Product Sticker Label");
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
    assert!(templates.iter().all(|t| t["type"] == "document"));
    let mut descriptions: Vec<&str> = templates
        .iter()
        .map(|t| t["description"].as_str().unwrap())
        .collect();
    descriptions.sort_unstable();
    assert_eq!(
        descriptions,
        vec!["A4 Invoice", "Credit Note", "Thermal Receipt"]
    );

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
    assert_eq!(templates[0]["description"], "Product Sticker Label");
    assert!(
        templates[0]["name"]
            .as_str()
            .unwrap()
            .starts_with("lbl_temp_")
    );
    assert_eq!(templates[0]["type"], "label");
    assert_eq!(templates[0]["data"]["reference"], "SKU-00042");
}

#[tokio::test]
async fn get_template_by_key_returns_that_template_with_data() {
    let app = common::spawn_app().await;

    let sticker_key: String = sqlx::query_scalar("SELECT key FROM templates WHERE description = ?")
        .bind("Product Sticker Label")
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
    assert_eq!(json["data"]["description"], "Product Sticker Label");
    assert!(
        json["data"]["name"]
            .as_str()
            .unwrap()
            .starts_with("lbl_temp_")
    );
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
    assert_eq!(names.len(), 4);
    assert!(
        names
            .iter()
            .all(|n| n.starts_with("doc_temp_") || n.starts_with("lbl_temp_"))
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
    common::copy_dir_recursive(std::path::Path::new("templates"), templates_root.path())
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

// ============================================================================
// Create (`POST /api/templates`)
// ============================================================================

/// A `TestApp` whose templates dir is a writable copy of the seed tree, so
/// `POST /api/templates` can drop files without touching this repo.
async fn spawn_with_writable_templates() -> (common::TestApp, tempfile::TempDir) {
    let root = tempfile::tempdir().expect("create temp templates root");
    common::copy_dir_recursive(std::path::Path::new("templates"), root.path())
        .expect("copy seed templates into temp dir");
    let app =
        common::spawn_app_with_templates_dir(root.path().to_str().expect("utf-8 temp path")).await;
    (app, root)
}

fn post_create(api_key: Option<&str>, body: serde_json::Value) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/templates")
        .header("content-type", "application/json");
    if let Some(key) = api_key {
        builder = builder.header("X-Internal-Api-Key", key);
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn create_template_requires_internal_api_key() {
    let (app, _root) = spawn_with_writable_templates().await;
    let response = app
        .router
        .oneshot(post_create(None, serde_json::json!({"source": "= Hi"})))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_template_writes_a_document_and_renders_it() {
    let (app, root) = spawn_with_writable_templates().await;

    let source = "#let data = sys.inputs\n= Invoice #data.at(\"ref\", default: \"\")\n";
    let response = app
        .router
        .clone()
        .oneshot(post_create(
            Some(&app.config.internal_api_key),
            serde_json::json!({
                "type": "document",
                "source": source,
                "schema": {
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "title": "Custom Report",
                    "type": "object",
                    "additionalProperties": true,
                    "required": ["ref"],
                    "properties": { "ref": { "type": "string" } }
                },
                "sample": { "ref": "R-1" }
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let json = body_json(response).await;
    let key = json["data"]["key"].as_str().unwrap().to_string();
    let name = json["data"]["name"].as_str().unwrap().to_string();
    assert!(name.starts_with("doc_temp_"), "got name {name}");
    assert_eq!(json["data"]["description"], "Custom Report");
    assert_eq!(json["data"]["isActive"], true);

    // Files landed in documents/.
    assert!(root.path().join(format!("documents/{name}.typ")).exists());
    assert!(
        root.path()
            .join(format!("documents/{name}.schema.json"))
            .exists()
    );

    // And it renders on the very next request — no restart.
    let response = app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/render/{key}"))
                .header("content-type", "application/json")
                .header("X-Internal-Api-Key", &app.config.internal_api_key)
                .body(Body::from(serde_json::json!({"ref": "R-9"}).to_string()))
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
async fn create_template_writes_a_label() {
    let (app, root) = spawn_with_writable_templates().await;
    let response = app
        .router
        .oneshot(post_create(
            Some(&app.config.internal_api_key),
            serde_json::json!({ "type": "label", "source": "= Label\n" }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let json = body_json(response).await;
    let name = json["data"]["name"].as_str().unwrap().to_string();
    assert!(name.starts_with("lbl_temp_"), "got name {name}");
    assert_eq!(json["data"]["type"], "label");
    assert!(root.path().join(format!("labels/{name}.typ")).exists());
}

#[tokio::test]
async fn create_template_rejects_empty_source() {
    let (app, _root) = spawn_with_writable_templates().await;
    let response = app
        .router
        .oneshot(post_create(
            Some(&app.config.internal_api_key),
            serde_json::json!({ "source": "   " }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body_json(response).await["code"], "TEMPLATE_CREATE_FAILED");
}

#[tokio::test]
async fn create_template_rejects_invalid_schema() {
    let (app, _root) = spawn_with_writable_templates().await;
    let response = app
        .router
        .oneshot(post_create(
            Some(&app.config.internal_api_key),
            serde_json::json!({
                "source": "= Hi\n",
                "schema": { "type": "not-a-real-type" }
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body_json(response).await["code"], "TEMPLATE_CREATE_FAILED");
}

#[tokio::test]
async fn create_template_that_fails_to_compile_is_rejected_and_rolled_back() {
    let (app, root) = spawn_with_writable_templates().await;
    let before: Vec<_> = std::fs::read_dir(root.path().join("documents"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name())
        .collect();

    let response = app
        .router
        .oneshot(post_create(
            Some(&app.config.internal_api_key),
            serde_json::json!({
                "source": "#let x = ( // unterminated\n",
                "sample": {}
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body_json(response).await["code"], "TEMPLATE_CREATE_FAILED");

    // Nothing new left behind on disk.
    let after: Vec<_> = std::fs::read_dir(root.path().join("documents"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name())
        .collect();
    assert_eq!(
        before.len(),
        after.len(),
        "rolled-back files should be gone"
    );
}
