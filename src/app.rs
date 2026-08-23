use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::{Json, Router, http::Method};
use sqlx::SqlitePool;
use tower_http::{
    LatencyUnit,
    cors::CorsLayer,
    trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer},
};
use tracing::Level;
use utoipa::OpenApi;
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    clients::render::RenderEngine,
    core::{config::Config, openapi::ApiDoc, response::ApiResponse},
    domain::HealthResponse,
    modules,
};

/// Shared application state injected into every handler via Axum's `State`
/// extractor. `Arc<Config>` because `Config` is read-only after startup;
/// `SqlitePool` is already an internally-`Arc`'d connection pool, so cloning
/// it per-request is cheap; the render engine is an `ArcSwap` rather than a
/// plain `Arc` because `POST /api/templates/sync` rebuilds it from disk at
/// runtime — readers (`load_full()`) always see one coherent fully-warmed
/// engine, never a half-built one.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: SqlitePool,
    pub render: Arc<ArcSwap<RenderEngine>>,
}

/// Assembles the full HTTP router: the top-level `/health` check, every
/// feature module nested under `/api/<module>`, Swagger UI, CORS, and
/// request tracing — this is the one place all of that gets wired together,
/// called once from `main.rs`.
pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers(tower_http::cors::Any);

    // Logs every request/response to the terminal (method, path, status,
    // latency) at INFO level, so nothing extra needs to be set (e.g.
    // RUST_LOG) to see request activity — tower_http's defaults for these
    // callbacks are DEBUG, which stays silent under the app's default filter.
    let trace = TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_request(DefaultOnRequest::new().level(Level::INFO))
        .on_response(
            DefaultOnResponse::new()
                .level(Level::INFO)
                .latency_unit(LatencyUnit::Millis),
        );

    use crate::core::constants::modules as mod_names;

    // Access control below is the single shared-secret model of
    // `core::middleware::auth`: routes meant only for the backend callers
    // (`render`, `documents`, `templates/sync`, all of `template-data`)
    // take the `InternalCaller` extractor and require `X-Internal-Api-Key`,
    // while genuinely-public reads (`GET /api/templates*`, health, Swagger)
    // take no extractor. There are no users/roles/tokens here — if a richer
    // auth model is ever needed, it must be a deliberate new addition, not
    // a half-extension of this one.
    let api_router: OpenApiRouter<AppState> = OpenApiRouter::new()
        .routes(routes!(health))
        .nest(
            &format!("/{}", mod_names::RENDER),
            modules::render::routes::router(state.config.max_render_body_bytes),
        )
        .nest(
            &format!("/{}", mod_names::TEMPLATES),
            modules::templates::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::DOCUMENTS),
            modules::documents::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::TEMPLATE_DATA),
            modules::template_data::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::BARCODES),
            modules::barcodes::routes::router(),
        )
        .nest(
            &format!("/{}", mod_names::QRCODES),
            modules::qrcodes::routes::router(),
        );

    let (router, openapi) = OpenApiRouter::<AppState>::with_openapi(ApiDoc::openapi())
        .nest("/api", api_router)
        .split_for_parts();

    router
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", openapi))
        .layer(cors)
        .layer(trace)
        .with_state(state)
}

#[utoipa::path(get, path = "/health", tag = "health", responses(
    (status = 200, description = "Service is up", body = ApiResponse<HealthResponse>)
))]
/// Liveness check — always returns 200 if the process is up and able to
/// handle a request at all. Doesn't touch SQLite or the render engine, so it
/// can't distinguish "server up, database down"; use a module's own status
/// route for that. Returns Cache-Control: no-store.
async fn health() -> (
    [(axum::http::HeaderName, &'static str); 1],
    Json<ApiResponse<HealthResponse>>,
) {
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(ApiResponse::success(
            HealthResponse {
                status: "ok".to_string(),
            },
            "Service is healthy",
        )),
    )
}
