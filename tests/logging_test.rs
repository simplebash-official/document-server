// Request + render logging (`core::logging`): an `X-Request-Id` from the
// backend is honoured and echoed, every request gets `http/request` +
// `http/response` lines inside a span carrying that id, and a render records
// `render/start` + `render/failed` with the same id. Captures the JSON log
// output in-process through a global subscriber, so this file stays a single
// test (one subscriber per test binary).

mod common;

use std::io::Write;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn lines_for(captured: &Captured, request_id: &str) -> Vec<serde_json::Value> {
    String::from_utf8(captured.0.lock().unwrap().clone())
        .unwrap()
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|l| {
            l["spans"].as_array().is_some_and(|spans| {
                spans
                    .iter()
                    .any(|s| s["request_id"].as_str() == Some(request_id))
            })
        })
        .collect()
}

#[tokio::test]
async fn requests_and_renders_are_logged_with_the_callers_request_id() {
    // SAFETY: set before any thread reads the logging settings, in a test
    // binary with a single test.
    unsafe {
        std::env::set_var("LOG_HTTP_BODIES", "true");
    }
    let captured = Captured::default();
    let writer = captured.clone();
    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(true)
        .with_writer(move || writer.clone())
        .with_max_level(tracing::Level::INFO)
        .init();

    let app = common::spawn_app().await;

    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .header("x-request-id", "req_backend-42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["x-request-id"], "req_backend-42");

    let health = lines_for(&captured, "req_backend-42");
    let response_line = health
        .iter()
        .find(|l| l["category"] == "http" && l["event"] == "response")
        .expect("http/response line");
    assert_eq!(response_line["status"], 200);

    // An unknown template: render/start then render/failed, same request id.
    let body = r#"{"customerName":"Kamal","apiKey":"should-not-appear"}"#;
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/render/tpl_does_not_exist")
                .header("x-request-id", "req_render-7")
                .header("x-internal-api-key", app.config.internal_api_key.as_str())
                .header("content-type", "application/json")
                .header("content-length", body.len())
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let render = lines_for(&captured, "req_render-7");
    let events: Vec<(&str, &str)> = render
        .iter()
        .map(|l| {
            (
                l["category"].as_str().unwrap_or(""),
                l["event"].as_str().unwrap_or(""),
            )
        })
        .collect();
    for expected in [
        ("http", "request"),
        ("render", "start"),
        ("render", "failed"),
        ("error", "client_error"),
        ("http", "response"),
    ] {
        assert!(
            events.contains(&expected),
            "missing {expected:?} in {events:?}"
        );
    }
    let request_line = render.iter().find(|l| l["event"] == "request").unwrap();
    let logged = request_line["body"].as_str().unwrap();
    assert!(
        logged.contains("Kamal") && logged.contains("[REDACTED]"),
        "{logged}"
    );
    assert!(!logged.contains("should-not-appear"));
}
