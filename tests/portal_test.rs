use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use document_server::{
    app::{self, AppState},
    clients::render::RenderEngine,
    core::config::Config,
};
use tower::ServiceExt;

#[tokio::test]
async fn portal_renders_at_root_and_api() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_lazy("sqlite::memory:")
        .expect("in-memory sqlite pool");

    let config = Config {
        database_url: "sqlite::memory:".to_string(),
        port: 8090,
        bind_addr: "127.0.0.1".to_string(),
        templates_dir: "templates".to_string(),
        fonts_dir: "fonts".to_string(),
        max_render_body_bytes: 1024 * 1024,
        internal_api_key: "test-internal-api-key".to_string(),
        remote_image_max_bytes: 5242880,
        remote_image_timeout_secs: 10,
        remote_image_fetch_enabled: true,
        app_env: "development".to_string(),
    };

    let render = RenderEngine::warm_up(&config.templates_dir, &config.fonts_dir)
        .expect("warm-up render engine");

    let state = AppState {
        config: Arc::new(config),
        db: pool,
        render: Arc::new(ArcSwap::from_pointee(render)),
    };
    let app = app::build_router(state);

    // 1. Test GET /
    let res = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let content_type = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert!(content_type.contains("text/html"));

    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(body_str.contains("SimpleBash Document Engine"));
    assert!(body_str.contains("telemetry-signal"));
    assert!(body_str.contains("uptime-ticker"));
    assert!(body_str.contains("Development"));

    // 2. Test GET /api
    let res_api = app
        .clone()
        .oneshot(Request::builder().uri("/api").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res_api.status(), StatusCode::OK);

    // 3. Test GET /api/health
    let res_health = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_health.status(), StatusCode::OK);
    let health_bytes = axum::body::to_bytes(res_health.into_body(), usize::MAX)
        .await
        .unwrap();
    let health_json: serde_json::Value = serde_json::from_slice(&health_bytes).unwrap();
    assert_eq!(health_json["data"]["status"], "ok");
    assert!(health_json["data"]["uptime_seconds"].is_number());
    assert_eq!(health_json["data"]["environment"], "development");
}
