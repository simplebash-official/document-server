// Full-stack, real SQLite (throwaway per-test file via `common::spawn_app`).
// Covers the real `GET /api/templates` / `GET /api/templates/{key}` read
// routes added in Phase 2 — `render_test.rs` already covers the render
// pipeline itself, this file is scoped to templates-as-a-resource.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn list_templates_returns_every_seeded_template() {
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
    // Sorted by name — the seed `templates/` directory has exactly these two.
    assert_eq!(names, vec!["invoice", "sticker"]);
    assert!(templates.iter().all(|t| t["isActive"] == true));
    assert!(
        templates
            .iter()
            .all(|t| t["key"].as_str().unwrap().starts_with("tpl_"))
    );
}

#[tokio::test]
async fn get_template_by_key_returns_that_template() {
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
