use std::sync::Arc;

use axum::Router;
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
}

/// Builds the real router against a throwaway SQLite file (deleted when the
/// returned `TestApp` drops) and a real, warmed-up `RenderEngine` (against
/// the real `templates/`/`fonts/` directories, resolved relative to the
/// crate root `cargo test` runs from). Every call gets its own database, so
/// tests are isolated from each other with no external service to run.
pub async fn spawn_app() -> TestApp {
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
        render: Arc::new(render),
    };

    TestApp {
        router: app::build_router(state),
        db,
        config,
        db_file,
    }
}
