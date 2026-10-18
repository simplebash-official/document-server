// Template metadata and visibility. A template is either a bundled (global)
// design synced from the shipped files at boot (`service::sync_bundled`) or a
// private package published through `modules::packages`. `service` is `pub`
// because `modules::render::service`, `modules::documents::service`,
// `modules::packages` and `main.rs` (a separate binary crate under the
// lib/bin split) call into it; `repository` stays private — nothing outside
// this module tree touches SQLite directly.
pub mod model;
mod repository;
pub mod routes;
pub mod service;
