//! Standardized error code strings used in `ErrorResponse` across all modules.

// Generic status codes — the default `code` for each `AppError` variant.
pub const NOT_FOUND: &str = "NOT_FOUND";
pub const VALIDATION_ERROR: &str = "VALIDATION_ERROR";
pub const INTERNAL_SERVER_ERROR: &str = "INTERNAL_SERVER_ERROR";
pub const UNAUTHORIZED: &str = "UNAUTHORIZED";

// Internal-caller auth (see `core::middleware::auth::InternalCaller`).
pub const INTERNAL_API_KEY_INVALID: &str = "INTERNAL_API_KEY_INVALID";
/// A caller-supplied `X-Tenant-Key` doesn't look like a tenant id (see
/// `core::middleware::auth::TenantKey`).
pub const TENANT_KEY_INVALID: &str = "TENANT_KEY_INVALID";

// Render pipeline (see spec §4, §6.2 and `modules::render::service`).
pub const TEMPLATE_NOT_FOUND: &str = "TEMPLATE_NOT_FOUND";
pub const RENDER_VALIDATION_FAILED: &str = "RENDER_VALIDATION_FAILED";
pub const PDF_EXPORT_FAILED: &str = "PDF_EXPORT_FAILED";
/// A render payload's `logoUrl` could not be downloaded (bad scheme,
/// non-image response, too large, unreachable) — see `clients::http`.
pub const REMOTE_IMAGE_FETCH_FAILED: &str = "REMOTE_IMAGE_FETCH_FAILED";

// Template authoring (see `modules::templates::service::create_template`).
pub const TEMPLATE_CREATE_FAILED: &str = "TEMPLATE_CREATE_FAILED";

// Documents (see `modules::documents::service`).
pub const DOCUMENT_NOT_FOUND: &str = "DOCUMENT_NOT_FOUND";

// Stored template data (see `modules::template_data::service`) — the
// per-template shared/static JSON blobs merged into render payloads.
pub const TEMPLATE_DATA_NOT_FOUND: &str = "TEMPLATE_DATA_NOT_FOUND";

// Barcodes & QR codes (see `modules::barcodes::service` and `modules::qrcodes::service`).
pub const BARCODE_GENERATION_FAILED: &str = "BARCODE_GENERATION_FAILED";
pub const QR_GENERATION_FAILED: &str = "QR_GENERATION_FAILED";
pub const INVALID_BARCODE_FORMAT: &str = "INVALID_BARCODE_FORMAT";
