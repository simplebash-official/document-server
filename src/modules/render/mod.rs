// The render endpoint itself: resolves a template, compiles it against the
// request body via `clients::render::RenderEngine`, and records a
// `documents` entry. `repository` stays private — nothing outside this
// module tree touches Typst directly. `service` is `pub(crate)`, not
// private: `modules::documents::service::reprint_document` (Phase 3's
// `GET /api/documents/{key}/pdf`) calls `service::compile_pdf` here so a
// reprint goes through the exact same Typst-compile-error -> `AppError`
// mapping `render_template` uses, instead of duplicating it.
mod repository;
pub mod routes;
pub(crate) mod service;
