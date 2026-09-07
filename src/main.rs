// Thin binary: load config, connect to SQLite, warm up the render engine,
// sync template metadata, build the router, serve. Each step below fails
// fast (log + `process::exit(1)`) rather than letting the server start in a
// half-working state — a bad `.env`, an unopenable database file, an
// unreadable templates/fonts directory, or an already-bound port should
// never look like a running server that then fails on the first real
// request.

use std::sync::Arc;

use arc_swap::ArcSwap;

use document_server::{
    app, app::AppState, clients, clients::render::RenderEngine, core::config::Config,
    modules::templates,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // `.ok()`: a missing `.env` is fine in prod (real env vars are already
    // set); `Config::from_env()` below is what actually enforces the
    // required variables are present.
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("document_server=info,tower_http=info,info")),
        )
        .init();

    let config = Config::from_env().unwrap_or_else(|err| {
        tracing::error!(%err, "invalid configuration");
        std::process::exit(1);
    });
    let port = config.port;
    let bind_addr = config.bind_addr.clone();

    tracing::info!(database_url = %config.database_url, "Opening SQLite database...");
    let db = clients::sqlite::connect(&config.database_url)
        .await
        .unwrap_or_else(|err| {
            tracing::error!(%err, "failed to open SQLite database");
            std::process::exit(1);
        });
    tracing::info!("SQLite database ready");

    tracing::info!(
        templates_dir = %config.templates_dir,
        fonts_dir = %config.fonts_dir,
        "Warming up render engine..."
    );
    let render =
        RenderEngine::warm_up(&config.templates_dir, &config.fonts_dir).unwrap_or_else(|err| {
            tracing::error!(%err, "failed to warm up render engine");
            std::process::exit(1);
        });
    tracing::info!(
        templates = ?render.known_templates(),
        "Render engine warmed up"
    );

    // Best-effort in spirit (each upsert only ever touches `is_active`/
    // `updated_at`, never clobbers hand-edited metadata — see
    // `templates::repository::upsert_template_by_name`), but a database
    // error here is still fatal: it means the connection just established
    // above is already unusable, not a transient/ignorable condition.
    templates::service::sync_templates_from_disk(&db, render.known_templates())
        .await
        .unwrap_or_else(|err| {
            tracing::error!(%err, "failed to sync templates from disk");
            std::process::exit(1);
        });

    let state = AppState {
        config: Arc::new(config),
        db,
        render: Arc::new(ArcSwap::from_pointee(render)),
    };
    let router = app::build_router(state);

    // Defaults to 0.0.0.0 so a container/host can route external traffic in;
    // `BIND_ADDR=127.0.0.1` restricts it to loopback (used by the Tauri
    // desktop bundle, where only the local backend calls this service).
    let listener = tokio::net::TcpListener::bind((bind_addr.as_str(), port))
        .await
        .unwrap_or_else(|err| {
            tracing::error!(%err, %bind_addr, "failed to bind listener");
            std::process::exit(1);
        });

    tracing::info!("Server started successfully on port {}", port);
    tracing::info!("   - API Base URL:  http://localhost:{}/api", port);
    tracing::info!("   - Swagger Docs:  http://localhost:{}/docs", port);
    tracing::info!(
        "   - OpenAPI Spec:  http://localhost:{}/api-docs/openapi.json",
        port
    );

    axum::serve(listener, router).await.unwrap_or_else(|err| {
        tracing::error!(%err, "server error");
        std::process::exit(1);
    });
}
