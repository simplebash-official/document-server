// Template metadata: one document per `.typ` file on disk, kept in sync by
// `service::sync_templates_from_disk`. `service` is `pub` (not merely
// `pub(crate)`) for two reasons: `modules::render::service` (a sibling
// module) needs `get_active_template_by_key`, and `main.rs` — a *separate
// binary crate* under the lib/bin split — needs `sync_templates_from_disk`
// directly at startup; a `pub(crate)` item in this library crate would be
// invisible to `main.rs` entirely. `repository` stays private — nothing
// outside this module tree touches SQLite directly.
pub mod model;
mod repository;
pub mod routes;
pub mod service;
