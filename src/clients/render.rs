// The embedded Typst compiler, built once at startup and shared read-only
// via `AppState` — the same "connect/build once, hand out a shared handle"
// shape as `clients::sqlite::connect`. Per-request compile orchestration
// (JSON -> Typst input, error translation) lives in
// `modules::render::repository::typst`, not here — this module only owns
// the engine's lifecycle: reading fonts/templates off disk once and
// building the `TypstEngine`.

use std::path::Path;

use typst::foundations::Dict;
use typst_as_lib::{TypstEngine, TypstTemplateCollection};
use typst_layout::PagedDocument;

/// Why `RenderEngine::warm_up` failed. A misconfigured `TEMPLATES_DIR`/
/// `FONTS_DIR` is a hard startup error — same fail-fast posture as
/// `Config::from_env()` — an operator should find out at boot, not on the
/// first render request.
#[derive(Debug, thiserror::Error)]
pub enum RenderEngineError {
    #[error("could not read fonts directory {path}: {source}")]
    FontsDirUnreadable {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error("could not read templates directory {path}: {source}")]
    TemplatesDirUnreadable {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
}

pub struct RenderEngine {
    engine: TypstEngine<TypstTemplateCollection>,
    // Every `.typ` filename stem found under `TEMPLATES_DIR` at warm-up
    // time — feeds `templates::service::sync_templates_from_disk`'s upsert
    // pass, so the `templates` collection and what's actually on disk never
    // drift.
    known_templates: Vec<String>,
}

impl RenderEngine {
    /// Builds the engine once: reads every font under `fonts_dir` into
    /// memory (`.fonts()` takes owned bytes — a genuine preload), points
    /// the engine at `templates_dir` via `with_file_system_resolver` (which
    /// resolves `.typ` files, and any assets they reference, lazily from
    /// disk by relative path — Typst's own internal caching, not a
    /// hand-rolled one, is what avoids repeat disk I/O per request), then
    /// runs one best-effort trial compile per known template purely to
    /// catch outright breakage (bad syntax, a missing asset, a bad font
    /// family) before the first real request. A trial compile failing
    /// because the template legitimately requires real input fields is
    /// expected — logged at `warn`, not treated as broken; the one true
    /// validation of "does this render successfully with realistic data" is
    /// `tests/render_test.rs`, not this pass.
    pub fn warm_up(
        templates_dir: impl AsRef<Path>,
        fonts_dir: impl AsRef<Path>,
    ) -> Result<Self, RenderEngineError> {
        let templates_dir = templates_dir.as_ref();
        let fonts_dir = fonts_dir.as_ref();

        let fonts = read_font_files(fonts_dir)?;
        let known_templates = read_template_stems(templates_dir)?;

        let engine = TypstEngine::builder()
            .fonts(fonts)
            .with_file_system_resolver(templates_dir.to_path_buf())
            .build();

        for name in &known_templates {
            let file_name = format!("{name}.typ");
            let warned: typst::diag::Warned<Result<PagedDocument, typst_as_lib::TypstAsLibError>> =
                engine.compile_with_input(file_name.as_str(), Dict::new());
            if let Err(err) = warned.output {
                tracing::warn!(
                    template = %name,
                    error = %err,
                    "template failed its startup trial compile (expected if it just needs real input data)"
                );
            }
        }

        Ok(Self {
            engine,
            known_templates,
        })
    }

    pub fn known_templates(&self) -> &[String] {
        &self.known_templates
    }

    /// Raw typst-as-lib call, untranslated (compile errors stay as
    /// `TypstAsLibError`). `pub(crate)` — only
    /// `modules::render::repository::typst` (same crate) needs it; that's
    /// where the `AppError` translation happens.
    pub(crate) fn compile(
        &self,
        template_name: &str,
        input: Dict,
    ) -> typst::diag::Warned<Result<PagedDocument, typst_as_lib::TypstAsLibError>> {
        let file_name = format!("{template_name}.typ");
        self.engine.compile_with_input(file_name.as_str(), input)
    }
}

fn read_font_files(dir: &Path) -> Result<Vec<Vec<u8>>, RenderEngineError> {
    let entries =
        std::fs::read_dir(dir).map_err(|source| RenderEngineError::FontsDirUnreadable {
            path: dir.to_path_buf(),
            source,
        })?;

    let mut fonts = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| RenderEngineError::FontsDirUnreadable {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.is_file() {
            let bytes =
                std::fs::read(&path).map_err(|source| RenderEngineError::FontsDirUnreadable {
                    path: dir.to_path_buf(),
                    source,
                })?;
            fonts.push(bytes);
        }
    }
    Ok(fonts)
}

fn read_template_stems(dir: &Path) -> Result<Vec<String>, RenderEngineError> {
    let entries =
        std::fs::read_dir(dir).map_err(|source| RenderEngineError::TemplatesDirUnreadable {
            path: dir.to_path_buf(),
            source,
        })?;

    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| RenderEngineError::TemplatesDirUnreadable {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("typ")
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        {
            names.push(stem.to_string());
        }
    }
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use typst::foundations::{Dict, IntoValue, Str, Value};

    use super::*;

    /// No database involved — exercises the real `templates/invoice.typ` +
    /// `fonts/` against the real Typst compiler, confirming `sys.inputs`
    /// threading and PDF export both actually work end to end.
    #[test]
    fn warm_up_and_compile_invoice_produces_a_pdf() {
        let engine = RenderEngine::warm_up("templates", "fonts").expect("warm_up should succeed");
        assert!(engine.known_templates().contains(&"invoice".to_string()));

        let mut items = Vec::new();
        let mut item = Dict::new();
        item.insert(Str::from("description"), "Widget".into_value());
        item.insert(Str::from("quantity"), Value::Int(2));
        item.insert(Str::from("unitPrice"), Value::Float(9.99));
        items.push(Value::Dict(item));

        let mut input = Dict::new();
        input.insert(Str::from("invoiceNumber"), "INV-0001".into_value());
        input.insert(Str::from("customerName"), "Jane Doe".into_value());
        input.insert(
            Str::from("items"),
            Value::Array(items.into_iter().collect()),
        );
        input.insert(Str::from("total"), Value::Float(19.98));

        let warned = engine.compile("invoice", input);
        let doc = warned.output.expect("invoice.typ should compile");

        let pdf_bytes =
            typst_pdf::pdf(&doc, &Default::default()).expect("pdf export should succeed");
        assert!(pdf_bytes.starts_with(b"%PDF-"));
    }
}
