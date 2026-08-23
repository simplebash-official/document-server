use utoipa::{
    Modify, OpenApi,
    openapi::security::{ApiKey, ApiKeyValue, SecurityScheme},
};

/// Root OpenAPI document. Paths are populated at router build time by
/// merging each module's `OpenApiRouter` (see `app::build_router`) — this
/// struct only carries top-level metadata. `render`/`documents` routes
/// reference the `internalApiKey` security scheme registered below by
/// `SecurityAddon`; `templates`/`barcodes`/`qrcodes` stay unauthenticated.
#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    info(
        title = "document-server API",
        description = "Standalone Typst-based document rendering service for jana2u-pos: renders a named template against a JSON payload and returns a PDF. The render/documents routes require an X-Internal-Api-Key header — jana2u-pos's backend is the only intended caller, the frontend never calls this service directly.",
        version = "0.1.0"
    ),
    tags(
        (name = "render", description = "Template rendering"),
        (name = "templates", description = "Template metadata"),
        (name = "documents", description = "Rendered document records"),
        (name = "template-data", description = "Stored shared/static template data merged into render payloads"),
        (name = "barcodes", description = "1D Barcode generation"),
        (name = "qrcodes", description = "2D QR Code generation"),
    )
)]
pub struct ApiDoc;

/// Registers the `internalApiKey` security scheme referenced by the
/// `render`/`documents` routes' `security(("internalApiKey" = []))`
/// annotations — mirrors jana2u-pos's backend `SecurityAddon`, just for a
/// single shared-secret header instead of a bearer JWT.
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "internalApiKey",
                SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-Internal-Api-Key"))),
            );
        }
    }
}
