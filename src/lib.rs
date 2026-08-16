// Library crate re-exporting every module so `main.rs` and `tests/*.rs`
// (each a separate crate) can share the same code via `document_server::...` —
// neither can reach `main.rs`'s `crate::` paths directly.
pub mod app;
pub mod clients;
pub mod core;
pub mod domain;
pub mod modules;
