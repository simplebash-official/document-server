//! Standardized error code strings used in `ErrorResponse` across all modules.

// Generic status codes — the default `code` for each `AppError` variant.
pub const NOT_FOUND: &str = "NOT_FOUND";
pub const VALIDATION_ERROR: &str = "VALIDATION_ERROR";
pub const INTERNAL_SERVER_ERROR: &str = "INTERNAL_SERVER_ERROR";

// Render pipeline (see spec §4, §6.2 and `modules::render::service`).
pub const TEMPLATE_NOT_FOUND: &str = "TEMPLATE_NOT_FOUND";
pub const RENDER_VALIDATION_FAILED: &str = "RENDER_VALIDATION_FAILED";
pub const PDF_EXPORT_FAILED: &str = "PDF_EXPORT_FAILED";
