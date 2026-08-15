// Typst-only orchestration for the render pipeline — no `AppError`, no
// SQLite. Kept as its own submodule (rather than inline in `service`) so the
// one place that touches `typst`/`typst_pdf` types directly stays isolated,
// same as every other module's `repository` isolates its one driver/
// framework dependency.
pub(crate) mod typst;
