// Records of rendered documents. `service` is `pub(crate)`, not `pub` —
// only the sibling `render` module (same crate) calls in, unlike
// `modules::templates` which `main.rs` also needs directly. `repository`
// stays private — nothing outside this module tree touches SQLite directly.
pub mod model;
mod repository;
pub mod routes;
pub(crate) mod service;
