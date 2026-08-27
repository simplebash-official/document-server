use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::{Router, http::header, response::IntoResponse, routing::get};
use document_server::{
    app, app::AppState, clients, clients::render::RenderEngine, core::config::Config,
    modules::templates,
};
use sqlx::SqlitePool;

pub struct TestApp {
    pub router: Router,
    #[allow(dead_code)]
    pub db: SqlitePool,
    #[allow(dead_code)]
    pub config: Arc<Config>,
    // Held only for its lifetime — the underlying file is deleted on drop.
    // Never read directly.
    #[allow(dead_code)]
    db_file: tempfile::NamedTempFile,
    // Present for `spawn_app_isolated_templates`: the writable copy of the
    // seed tree, kept alive (and auto-deleted) for the app's lifetime.
    #[allow(dead_code)]
    pub templates_dir: Option<tempfile::TempDir>,
}

/// Recursively copies `src` into `dst` (used to give a test its own writable
/// copy of the seed `templates/` tree).
#[allow(dead_code)]
pub fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
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

/// `spawn_app`, but against a private writable copy of `templates/` — for
/// tests that write into the tree (staged remote images, `POST /api/templates`)
/// and must not race other tests or dirty the repo.
#[allow(dead_code)]
pub async fn spawn_app_isolated_templates() -> TestApp {
    let root = tempfile::tempdir().expect("create temp templates root");
    copy_dir_recursive(std::path::Path::new("templates"), root.path())
        .expect("copy seed templates into temp dir");
    let mut app =
        spawn_app_with_templates_dir(root.path().to_str().expect("utf-8 temp path")).await;
    app.templates_dir = Some(root);
    app
}

/// Builds the real router against a throwaway SQLite file (deleted when the
/// returned `TestApp` drops) and a real, warmed-up `RenderEngine` (against
/// the real `templates/`/`fonts/` directories, resolved relative to the
/// crate root `cargo test` runs from). Every call gets its own database, so
/// tests are isolated from each other with no external service to run.
pub async fn spawn_app() -> TestApp {
    spawn_app_with_templates_dir("templates").await
}

/// Same as `spawn_app`, but pointing the engine (and the sync endpoint) at a
/// different templates directory — used by tests that add/remove `.typ`
/// files at runtime to prove `POST /api/templates/sync` picks them up
/// without touching this repo's real `templates/` tree.
pub async fn spawn_app_with_templates_dir(templates_dir: &str) -> TestApp {
    dotenvy::dotenv().ok();

    if std::env::var("INTERNAL_API_KEY").is_err() {
        // SAFETY: Only invoked at test process startup before multithreaded operations.
        unsafe {
            std::env::set_var("INTERNAL_API_KEY", "test-internal-api-key");
        }
    }

    let mut config = Config::from_env().expect("invalid configuration for test run");
    let db_file = tempfile::NamedTempFile::new().expect("create temp sqlite file");
    config.database_url = format!("sqlite://{}", db_file.path().display());
    config.templates_dir = templates_dir.to_string();

    let db = clients::sqlite::connect(&config.database_url)
        .await
        .expect("failed to open test SQLite database");

    let render = RenderEngine::warm_up(&config.templates_dir, &config.fonts_dir)
        .expect("failed to warm up render engine for test run");

    templates::service::sync_templates_from_disk(&db, render.known_templates())
        .await
        .expect("failed to sync templates from disk for test run");

    let config = Arc::new(config);
    let state = AppState {
        config: config.clone(),
        db: db.clone(),
        render: Arc::new(ArcSwap::from_pointee(render)),
    };

    TestApp {
        router: app::build_router(state),
        db,
        config,
        db_file,
        templates_dir: None,
    }
}

/// A 1×1 transparent PNG — the smallest thing `fetch_image` accepts and
/// Typst can decode.
#[allow(dead_code)]
pub const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xfc, 0xcf, 0xc0, 0x50,
    0x0f, 0x00, 0x04, 0x85, 0x01, 0x80, 0x84, 0xa9, 0x8c, 0x21, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// A throwaway localhost HTTP server for exercising the `logoUrl` fetch path
/// without reaching the internet. Routes:
///   `GET /logo.png`     → the tiny PNG above (`image/png`)
///   `GET /not-an-image` → `text/html` (rejected: not an image)
///   `GET /huge`         → `image/png` with a 6 MiB body (rejected: over cap)
/// Returns the base URL (e.g. `http://127.0.0.1:54321`); the server task is
/// aborted when the returned guard drops.
#[allow(dead_code)]
pub async fn spawn_image_server() -> ImageServer {
    async fn logo() -> impl IntoResponse {
        ([(header::CONTENT_TYPE, "image/png")], TINY_PNG)
    }
    async fn not_an_image() -> impl IntoResponse {
        ([(header::CONTENT_TYPE, "text/html")], "<html>nope</html>")
    }
    async fn huge() -> impl IntoResponse {
        (
            [(header::CONTENT_TYPE, "image/png")],
            vec![0u8; 6 * 1024 * 1024],
        )
    }

    let router = Router::new()
        .route("/logo.png", get(logo))
        .route("/not-an-image", get(not_an_image))
        .route("/huge", get(huge));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test image server");
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    ImageServer {
        base_url: format!("http://{addr}"),
        handle,
    }
}

#[allow(dead_code)]
pub struct ImageServer {
    pub base_url: String,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for ImageServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
