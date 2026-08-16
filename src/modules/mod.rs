// Feature modules, one per concern: `render` (the render endpoint itself),
// `templates` (template metadata), `documents` (rendered-document records).
// Each follows the same `routes.rs -> service -> repository` layering — see
// each module's own `mod.rs` for its specific visibility rules.
pub mod barcodes;
pub mod documents;
pub mod qrcodes;
pub mod render;
pub mod templates;
