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
        .route("/", axum::routing::get(portal))
        .route("/api", axum::routing::get(portal))
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", openapi))
        .layer(cors)
        .layer(trace)
        // Outermost: every layer below and every handler log line runs
        // inside this request's span (request_id / caller).
        .layer(axum::middleware::from_fn(
            crate::core::logging::request::log_requests,
        ))
        .with_state(state)
}

const PORTAL_HTML: &str = include_str!("portal.html");

static SERVER_STARTED_AT: std::sync::LazyLock<std::time::Instant> =
    std::sync::LazyLock::new(std::time::Instant::now);

pub fn server_uptime_seconds() -> u64 {
    SERVER_STARTED_AT.elapsed().as_secs()
}

async fn portal(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> (
    [(axum::http::HeaderName, &'static str); 1],
    axum::response::Html<String>,
) {
    let uptime = server_uptime_seconds();
    let version = env!("CARGO_PKG_VERSION");
    let env_name = &state.config.app_env;
    let env_class = if env_name.eq_ignore_ascii_case("production") || env_name.eq_ignore_ascii_case("prod") {
        "env-prod"
    } else {
        "env-dev"
    };

    let display_env = match env_name.to_lowercase().as_str() {
        "production" | "prod" => "Production".to_string(),
        "development" | "dev" => "Development".to_string(),
        "staging" | "stage" => "Staging".to_string(),
        "test" | "testing" => "Test".to_string(),
        s => {
            let mut chars = s.chars();
            match chars.next() {
                None => "Development".to_string(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        }
    };

    let rendered = PORTAL_HTML
        .replace("__VERSION__", version)
        .replace("__ENVIRONMENT__", &display_env)
        .replace("__ENV_CLASS__", env_class)
        .replace("__INITIAL_UPTIME__", &uptime.to_string());

    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        axum::response::Html(rendered),
    )
}

#[utoipa::path(get, path = "/health", tag = "health", responses(
    (status = 200, description = "Service is up", body = ApiResponse<HealthResponse>)
))]
/// Liveness check — returns 200 with service health status, live uptime, environment, and version.
async fn health(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> (
    [(axum::http::HeaderName, &'static str); 1],
    Json<ApiResponse<HealthResponse>>,
) {
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(ApiResponse::success(
            HealthResponse {
                status: "ok".to_string(),
                uptime_seconds: Some(server_uptime_seconds()),
                environment: Some(state.config.app_env.clone()),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            },
            "Service is healthy",
        )),
    )
}
