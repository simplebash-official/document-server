// Builds the real router against a throwaway SQLite file (no external
// service needed) plus a real, warmed-up `RenderEngine` (that part does need
// the real `templates/`/`fonts/` directories on disk) — for asserting
// routing/OpenAPI-doc wiring, not data. Deliberately not `sqlite::memory:`:
// `SqlitePoolOptions` can hand out more than one physical connection, and
// each connection to `:memory:` (without shared-cache mode) is its own
// separate empty database — a file avoids that trap entirely.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use pdf_server::{
    app, app::AppState, clients, clients::render::RenderEngine, core::config::Config,
};
use tower::ServiceExt;

async fn build_test_app() -> axum::Router {
    dotenvy::dotenv().ok();
    let mut config = Config::from_env().expect("invalid configuration for test run");
    let db_file = tempfile::NamedTempFile::new().expect("create temp sqlite file");
    config.database_url = format!("sqlite://{}", db_file.path().display());
    // Leaked deliberately: this file only needs to outlive the test process
    // (which is short-lived and single-purpose), and `Config`/`AppState`
    // have no room for a file handle to tag along.
    std::mem::forget(db_file);

    let db = clients::sqlite::connect(&config.database_url)
        .await
        .expect("failed to open test SQLite database");

    let render = RenderEngine::warm_up(&config.templates_dir, &config.fonts_dir)
        .expect("failed to warm up render engine for test run");

    let state = AppState {
        config: Arc::new(config),
        db,
        render: Arc::new(render),
    };
    app::build_router(state)
}

#[tokio::test]
async fn openapi_json_lists_all_module_paths() {
    let router = build_test_app().await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/api-docs/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let paths = json["paths"].as_object().unwrap();

    for expected in [
        "/api/health",
        "/api/render",
        "/api/render/{templateKey}",
        "/api/templates",
        "/api/templates/{key}",
        "/api/documents",
        "/api/documents/{key}",
        "/api/documents/{key}/pdf",
        "/api/barcodes",
        "/api/barcodes/generate",
        "/api/qrcodes",
        "/api/qrcodes/generate",
    ] {
        assert!(paths.contains_key(expected), "missing path: {expected}");
    }
}

#[tokio::test]
async fn render_endpoint_is_documented_as_pdf_response() {
    let router = build_test_app().await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/api-docs/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let responses = &json["paths"]["/api/render/{templateKey}"]["post"]["responses"]["200"];
    assert!(
        responses["content"]
            .as_object()
            .is_some_and(|c| c.contains_key("application/pdf")),
        "render endpoint's 200 response isn't documented as application/pdf: {responses}"
    );
}

#[tokio::test]
async fn reprint_endpoint_is_documented_as_pdf_response() {
    let router = build_test_app().await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/api-docs/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let responses = &json["paths"]["/api/documents/{key}/pdf"]["get"]["responses"]["200"];
    assert!(
        responses["content"]
            .as_object()
            .is_some_and(|c| c.contains_key("application/pdf")),
        "reprint endpoint's 200 response isn't documented as application/pdf: {responses}"
    );
}

#[tokio::test]
async fn swagger_ui_is_mounted() {
    let router = build_test_app().await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/docs/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
