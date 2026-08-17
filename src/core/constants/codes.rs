//! Standardized error code strings used in `ErrorResponse` across all modules.

// Generic status codes — the default `code` for each `AppError` variant.
pub const NOT_FOUND: &str = "NOT_FOUND";
pub const VALIDATION_ERROR: &str = "VALIDATION_ERROR";
pub const INTERNAL_SERVER_ERROR: &str = "INTERNAL_SERVER_ERROR";
pub const UNAUTHORIZED: &str = "UNAUTHORIZED";

// Internal-caller auth (see `core::middleware::auth::InternalCaller`).
pub const INTERNAL_API_KEY_INVALID: &str = "INTERNAL_API_KEY_INVALID";

// Render pipeline (see spec §4, §6.2 and `modules::render::service`).
pub const TEMPLATE_NOT_FOUND: &str = "TEMPLATE_NOT_FOUND";
pub const RENDER_VALIDATION_FAILED: &str = "RENDER_VALIDATION_FAILED";
pub const PDF_EXPORT_FAILED: &str = "PDF_EXPORT_FAILED";

// Documents (see `modules::documents::service`).
pub const DOCUMENT_NOT_FOUND: &str = "DOCUMENT_NOT_FOUND";

// Barcodes & QR codes (see `modules::barcodes::service` and `modules::qrcodes::service`).
pub const BARCODE_GENERATION_FAILED: &str = "BARCODE_GENERATION_FAILED";
pub const QR_GENERATION_FAILED: &str = "QR_GENERATION_FAILED";
pub const INVALID_BARCODE_FORMAT: &str = "INVALID_BARCODE_FORMAT";
