use utoipa::OpenApi;

/// Root OpenAPI document. Paths are populated at router build time by
/// merging each module's `OpenApiRouter` (see `app::build_router`) — this
/// struct only carries top-level metadata. Unlike jana2u-pos's backend,
/// there is no `SecurityAddon`/`bearerAuth` scheme here — this service has
/// no authentication layer, so there is no security scheme for any route to
/// reference.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "pdf-server API",
        description = "Standalone Typst-based PDF rendering service for jana2u-pos: renders a named template against a JSON payload and returns a PDF. No authentication — every route is open.",
        version = "0.1.0"
    ),
    tags(
        (name = "render", description = "Template rendering"),
        (name = "templates", description = "Template metadata"),
        (name = "documents", description = "Rendered document records"),
    )
)]
pub struct ApiDoc;
