// The embedded Typst compiler, built once at startup and shared read-only
// via `AppState` — the same "connect/build once, hand out a shared handle"
// shape as `clients::sqlite::connect`. Per-request compile orchestration
// (JSON -> Typst input, error translation) lives in
// `modules::render::repository::typst`, not here — this module only owns
// the engine's lifecycle: reading fonts/templates off disk once and
// building the `TypstEngine`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use typst::foundations::Dict;
use typst_as_lib::{TypstEngine, TypstTemplateCollection};
use typst_layout::PagedDocument;

use crate::domain::templates::TemplateType;

/// Filename prefix for a per-render temp image (a downloaded `logoUrl`)
/// staged next to a template. Dot-prefixed so `scan_templates` skips it and
/// so `warm_up`'s sweep can recognise and delete any left by a crash.
pub const REMOTE_IMAGE_PREFIX: &str = ".rimg_";

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

/// Metadata about a template discovered on disk during startup scan.
#[derive(Debug, Clone)]
pub struct DiscoveredTemplate {
    pub name: String,
    pub file_path: String,
    pub template_type: TemplateType,
    pub sample_data: Option<serde_json::Value>,
    /// Parsed `<name>.schema.json` sidecar — the template's machine-readable
    /// input contract (JSON Schema; drafts auto-detected from `$schema`).
    /// `None` when no sidecar exists: that template renders without payload
    /// validation, exactly as it did before schemas were introduced.
    pub data_schema: Option<serde_json::Value>,
    /// Human-readable label, taken from the schema sidecar's `title`. Since a
    /// template's `name` is now an opaque generated id (`doc_temp_<nanoid>`),
    /// this is what an operator/integration reads to tell templates apart —
    /// it lands in the `templates.description` column. `None` when there is
    /// no schema sidecar or it has no `title`.
    pub description: Option<String>,
}

pub struct RenderEngine {
    engine: TypstEngine<TypstTemplateCollection>,
    // Every template found under `TEMPLATES_DIR` at warm-up time
    // — feeds `templates::service::sync_templates_from_disk`'s upsert pass.
    known_templates: Vec<DiscoveredTemplate>,
    // Fast lookup from template name to relative file path
    template_paths: HashMap<String, String>,
    // The templates root this engine was built against — reused by
    // `compile_once` to build a throwaway engine for a single render.
    templates_dir: PathBuf,
    // The raw font bytes read at warm-up, kept so `compile_once` can build a
    // one-shot engine without re-reading `fonts/` from disk. `Arc` so that
    // clone is a refcount bump, not a copy of the (multi-MB) font data.
    fonts: Arc<Vec<Vec<u8>>>,
}

impl RenderEngine {
    /// Builds the engine once: reads every font under `fonts_dir` into
    /// memory (`.fonts()` takes owned bytes — a genuine preload), points
    /// the engine at `templates_dir` via `with_file_system_resolver` (which
    /// resolves `.typ` files, and any assets they reference, from disk by
    /// relative path), then runs one best-effort trial compile per known
    /// template purely to catch outright breakage (bad syntax, a missing asset,
    /// a bad font family) before the first real request.
    pub fn warm_up(
        templates_dir: impl AsRef<Path>,
        fonts_dir: impl AsRef<Path>,
    ) -> Result<Self, RenderEngineError> {
        let templates_dir = templates_dir.as_ref();
        let fonts_dir = fonts_dir.as_ref();

        let fonts = Arc::new(read_font_files(fonts_dir)?);
        let known_templates = scan_templates(templates_dir)?;

        // Delete any per-render temp images a previous process left behind
        // (crash between staging and cleanup). Best-effort — a failure here
        // must not block startup.
        sweep_remote_images(templates_dir);

        let mut template_paths = HashMap::new();
        for tpl in &known_templates {
            template_paths.insert(tpl.name.clone(), tpl.file_path.clone());
        }

        let engine = TypstEngine::builder()
            .fonts((*fonts).clone())
            .with_file_system_resolver(templates_dir.to_path_buf())
            .build();

        for tpl in &known_templates {
            let warned: typst::diag::Warned<Result<PagedDocument, typst_as_lib::TypstAsLibError>> =
                engine.compile_with_input(&*tpl.file_path, Dict::new());
            if let Err(err) = warned.output {
                // `debug`, not `warn`: a template that reads any field off
                // its input will *always* fail this empty-`Dict` probe —
                // that's the normal, expected case for every real template.
                tracing::debug!(
                    template = %tpl.name,
                    path = %tpl.file_path,
                    error = %err,
                    "template failed its startup trial compile (expected if it just needs real input data)"
                );
            }
        }

        Ok(Self {
            engine,
            known_templates,
            template_paths,
            templates_dir: templates_dir.to_path_buf(),
            fonts,
        })
    }

    pub fn known_templates(&self) -> &[DiscoveredTemplate] {
        &self.known_templates
    }

    /// Relative path (from the templates root) of the `.typ` for `name`, or
    /// `None` if unknown. `compile_once` needs this to name the main file.
    pub(crate) fn template_rel_path(&self, name: &str) -> Option<&str> {
        self.template_paths.get(name).map(String::as_str)
    }

    /// Compiles `main_rel_path` through a **freshly built, single-use**
    /// engine rooted at this engine's templates dir. Used only for a render
    /// that staged a downloaded image on disk: the throwaway engine — and
    /// the unbounded file cache `with_file_system_resolver` gives it — is
    /// dropped when this returns, so that image never lands in the shared
    /// engine's cache (which would keep it in memory for the process's life).
    /// Fonts are reused from the shared engine's preloaded bytes.
    pub(crate) fn compile_once(
        &self,
        main_rel_path: &str,
        input: Dict,
    ) -> typst::diag::Warned<Result<PagedDocument, typst_as_lib::TypstAsLibError>> {
        let engine = TypstEngine::builder()
            .fonts((*self.fonts).clone())
            .with_file_system_resolver(self.templates_dir.clone())
            .build();
        engine.compile_with_input(main_rel_path, input)
    }

    /// Compiles a template by name with given input dictionary.
    pub(crate) fn compile(
        &self,
        template_name: &str,
        input: Dict,
    ) -> typst::diag::Warned<Result<PagedDocument, typst_as_lib::TypstAsLibError>> {
        let file_path = self
            .template_paths
            .get(template_name)
            .cloned()
            .unwrap_or_else(|| format!("{template_name}.typ"));
        self.engine.compile_with_input(&*file_path, input)
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
        // Only font files: the directory also carries the fonts' licence
        // texts (e.g. `OFL.txt`), which must ship alongside them.
        let is_font = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "ttf" | "otf" | "ttc" | "otc"
                )
            });
        if path.is_file() && is_font {
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

/// Scans `TEMPLATES_DIR` for `.typ` files in `documents/`, `labels/`, or root.
/// Skips `lib/` and non-typ files.
fn scan_templates(dir: &Path) -> Result<Vec<DiscoveredTemplate>, RenderEngineError> {
    let mut templates = Vec::new();

    let entries =
        std::fs::read_dir(dir).map_err(|source| RenderEngineError::TemplatesDirUnreadable {
            path: dir.to_path_buf(),
            source,
        })?;

    for entry in entries {
        let entry = entry.map_err(|source| RenderEngineError::TemplatesDirUnreadable {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        if file_name_str == "lib" || file_name_str.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            let sub_type = if file_name_str == "labels" {
                TemplateType::Label
            } else {
                TemplateType::Document
            };

            let sub_entries = std::fs::read_dir(&path).map_err(|source| {
                RenderEngineError::TemplatesDirUnreadable {
                    path: path.clone(),
                    source,
                }
            })?;

            for sub_entry in sub_entries {
                let sub_entry =
                    sub_entry.map_err(|source| RenderEngineError::TemplatesDirUnreadable {
                        path: path.clone(),
                        source,
                    })?;
                let sub_path = sub_entry.path();
                if sub_path.extension().and_then(|ext| ext.to_str()) == Some("typ")
                    && let Some(stem) = sub_path.file_stem().and_then(|s| s.to_str())
                {
                    let (sample_data, data_schema) = read_sidecars(&sub_path);
                    let description = schema_title(data_schema.as_ref());

                    templates.push(DiscoveredTemplate {
                        name: stem.to_string(),
                        file_path: format!("{file_name_str}/{stem}.typ"),
                        template_type: sub_type,
                        sample_data,
                        data_schema,
                        description,
                    });
                }
            }
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("typ")
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        {
            let (sample_data, data_schema) = read_sidecars(&path);
            let description = schema_title(data_schema.as_ref());

            templates.push(DiscoveredTemplate {
                name: stem.to_string(),
                file_path: format!("{stem}.typ"),
                template_type: TemplateType::Document,
                sample_data,
                data_schema,
                description,
            });
        }
    }

    templates.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(templates)
}

/// Reads a `.typ` file's optional sidecars: `<stem>.json` (sample data, the
/// long-standing convention) and `<stem>.schema.json` (the machine-readable
/// input contract). Both are best-effort — an unparseable sidecar is treated
/// as absent rather than failing startup, same tolerance the sample-data
/// sidecar always had.
fn read_sidecars(typ_path: &Path) -> (Option<serde_json::Value>, Option<serde_json::Value>) {
    let sample_data = std::fs::read_to_string(typ_path.with_extension("json"))
        .ok()
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok());
    let data_schema = std::fs::read_to_string(typ_path.with_extension("schema.json"))
        .ok()
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok());

    (sample_data, data_schema)
}

/// Best-effort delete of every `REMOTE_IMAGE_PREFIX`-named temp image under
/// `dir` and its immediate subdirectories (`documents/`, `labels/`, …). Runs
/// at warm-up to clear anything a crashed render left behind; any IO error
/// is swallowed — a stale temp file is harmless, a failed startup is not.
fn sweep_remote_images(dir: &Path) {
    fn sweep_one(dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(REMOTE_IMAGE_PREFIX)
            {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    sweep_one(dir);
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                sweep_one(&entry.path());
            }
        }
    }
}

/// Pulls a template's human label out of its schema sidecar's `title` field.
/// Blank/whitespace-only titles are treated as absent.
fn schema_title(data_schema: Option<&serde_json::Value>) -> Option<String> {
    data_schema
        .and_then(|schema| schema.get("title"))
        .and_then(|title| title.as_str())
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use typst::foundations::{Dict, IntoValue, Str, Value};

    use super::*;

    /// The sticker template's own sidecar sample (a 50x30mm layout, 3 stickers).
    fn sticker_sample() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../templates/labels/lbl_temp_qklcWIolwoFFN3xk.json"
        ))
        .expect("sticker sample is valid JSON")
    }

    fn sticker_input() -> Dict {
        crate::modules::render::repository::typst::json_to_dict(sticker_sample())
    }

    /// Templates on disk have opaque generated names now; a test that wants
    /// "the thermal receipt" resolves it through the human label carried in
    /// the schema `title` (surfaced as `DiscoveredTemplate::description`).
    fn discovered<'a>(engine: &'a RenderEngine, title: &str) -> &'a DiscoveredTemplate {
        engine
            .known_templates()
            .iter()
            .find(|t| t.description.as_deref() == Some(title))
            .unwrap_or_else(|| panic!("no discovered template titled {title:?}"))
    }

    #[test]
    fn warm_up_and_compile_thermal_receipt_produces_a_pdf() {
        let engine = RenderEngine::warm_up("templates", "fonts").expect("warm_up should succeed");
        let template = discovered(&engine, "Thermal Receipt");
        assert_eq!(template.template_type, TemplateType::Document);
        let template_name = template.name.clone();

        let mut items = Vec::new();
        let mut item = Dict::new();
        item.insert(Str::from("name"), "Widget".into_value());
        item.insert(Str::from("quantity"), Value::Int(2));
        item.insert(Str::from("unitPriceCents"), Value::Int(999));
        item.insert(Str::from("discountCents"), Value::Int(0));
        item.insert(Str::from("totalCents"), Value::Int(1998));
        items.push(Value::Dict(item));

        let mut input = Dict::new();
        input.insert(Str::from("paperWidthMm"), Value::Int(80));
        input.insert(Str::from("invoiceNumber"), "INV-0001".into_value());
        input.insert(Str::from("cashierName"), "Jane Doe".into_value());
        input.insert(
            Str::from("items"),
            Value::Array(items.into_iter().collect()),
        );
        input.insert(Str::from("subtotalCents"), Value::Int(1998));
        input.insert(Str::from("totalCents"), Value::Int(1998));
        input.insert(Str::from("paymentMethod"), "cash".into_value());
        input.insert(Str::from("tenderedAmountCents"), Value::Int(2000));
        input.insert(Str::from("changeDueCents"), Value::Int(2));

        let warned = engine.compile(&template_name, input);
        let doc = warned
            .output
            .expect("thermal receipt template should compile");

        let pdf_bytes =
            typst_pdf::pdf(&doc, &Default::default()).expect("pdf export should succeed");
        assert!(pdf_bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn warm_up_and_compile_a4_invoice_produces_a_pdf() {
        let engine = RenderEngine::warm_up("templates", "fonts").expect("warm_up should succeed");
        let template = discovered(&engine, "A4 Invoice");
        assert_eq!(template.template_type, TemplateType::Document);
        let template_name = template.name.clone();

        let mut items = Vec::new();
        let mut item = Dict::new();
        item.insert(Str::from("name"), "Widget".into_value());
        item.insert(Str::from("quantity"), Value::Int(2));
        item.insert(Str::from("unitPriceCents"), Value::Int(999));
        item.insert(Str::from("discountCents"), Value::Int(0));
        item.insert(Str::from("totalCents"), Value::Int(1998));
        items.push(Value::Dict(item));

        let mut input = Dict::new();
        input.insert(Str::from("invoiceNumber"), "INV-0001".into_value());
        input.insert(Str::from("cashierName"), "Jane Doe".into_value());
        input.insert(Str::from("status"), "paid".into_value());
        input.insert(
            Str::from("items"),
            Value::Array(items.into_iter().collect()),
        );
        input.insert(Str::from("subtotalCents"), Value::Int(1998));
        input.insert(Str::from("totalCents"), Value::Int(1998));
        input.insert(Str::from("paymentMethod"), "cash".into_value());

        let warned = engine.compile(&template_name, input);
        let doc = warned.output.expect("a4 invoice template should compile");

        let pdf_bytes =
            typst_pdf::pdf(&doc, &Default::default()).expect("pdf export should succeed");
        assert!(pdf_bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn warm_up_and_compile_sticker_produces_a_pdf() {
        let engine = RenderEngine::warm_up("templates", "fonts").expect("warm_up should succeed");
        let template = discovered(&engine, "Product Sticker Label");
        assert_eq!(template.template_type, TemplateType::Label);
        let template_name = template.name.clone();
        // The vendored library files must never be mistaken for templates.
        assert!(!engine.known_templates().iter().any(|t| t.name == "lib"));

        let warned = engine.compile(&template_name, sticker_input());
        let doc = warned.output.expect("sticker template should compile");

        // The sample prints 2 + 1 stickers, one page each, sized by the payload.
        assert_eq!(doc.pages().len(), 3);
        let size = doc.pages()[0].frame.size();
        assert!(
            (size.x.to_mm() - 50.0).abs() < 0.01,
            "width {}",
            size.x.to_mm()
        );
        assert!(
            (size.y.to_mm() - 30.0).abs() < 0.01,
            "height {}",
            size.y.to_mm()
        );

        let pdf_bytes =
            typst_pdf::pdf(&doc, &Default::default()).expect("pdf export should succeed");
        assert!(pdf_bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn sticker_page_size_follows_the_payload() {
        let engine = RenderEngine::warm_up("templates", "fonts").expect("warm_up should succeed");
        let name = discovered(&engine, "Product Sticker Label").name.clone();
        let mut sample = sticker_sample();
        sample["widthMm"] = serde_json::json!(58);
        sample["heightMm"] = serde_json::json!(40);
        sample["elements"][1]["widthMm"] = serde_json::json!(54);

        let doc = engine
            .compile(
                &name,
                crate::modules::render::repository::typst::json_to_dict(sample),
            )
            .output
            .expect("a resized sticker should compile");
        let size = doc.pages()[0].frame.size();
        assert!((size.x.to_mm() - 58.0).abs() < 0.01);
        assert!((size.y.to_mm() - 40.0).abs() < 0.01);
    }

    /// Font sizes (pt) of every run of text in `frame` whose text contains `needle`.
    fn text_sizes(frame: &typst::layout::Frame, needle: &str) -> Vec<f64> {
        let mut sizes = Vec::new();
        for (_, item) in frame.items() {
            match item {
                typst::layout::FrameItem::Group(group) => {
                    sizes.extend(text_sizes(&group.frame, needle))
                }
                typst::layout::FrameItem::Text(text) if text.text.contains(needle) => {
                    sizes.push(text.size.to_pt())
                }
                _ => {}
            }
        }
        sizes
    }

    /// Compiles the sample with its first sticker renamed and returns that page.
    fn first_sticker_named(name: &str) -> PagedDocument {
        let engine = RenderEngine::warm_up("templates", "fonts").expect("warm_up should succeed");
        let template = discovered(&engine, "Product Sticker Label").name.clone();
        let mut sample = sticker_sample();
        sample["labels"][0]["name"] = serde_json::json!(name);
        engine
            .compile(
                &template,
                crate::modules::render::repository::typst::json_to_dict(sample),
            )
            .output
            .expect("the sticker should compile")
    }

    #[test]
    fn sticker_text_that_fits_keeps_its_own_size() {
        // The sample's name element is 8pt bold in a 46 x 6 mm box.
        let doc = first_sticker_named("Cable");
        let sizes = text_sizes(&doc.pages()[0].frame, "Cable");
        assert!(!sizes.is_empty(), "the name should be printed");
        assert!(
            sizes.iter().all(|size| (size - 8.0).abs() < 0.01),
            "{sizes:?}"
        );
    }

    #[test]
    fn sticker_text_too_long_for_its_box_shrinks_instead_of_being_cut() {
        let doc = first_sticker_named(
            "Samsung Galaxy A15 Silicone Back Cover with Camera Protection Navy Blue",
        );
        let sizes = text_sizes(&doc.pages()[0].frame, "Samsung");
        assert!(!sizes.is_empty(), "the start of the name should be printed");
        assert!(
            sizes.iter().all(|size| *size < 8.0 && *size >= 4.0),
            "{sizes:?}"
        );
        // The end of the name is on the label too, not clipped away.
        assert!(!text_sizes(&doc.pages()[0].frame, "Blue").is_empty());
    }

    #[test]
    fn sticker_text_never_shrinks_below_the_smallest_printable_size() {
        let doc = first_sticker_named(&"Very long item name ".repeat(40));
        let sizes = text_sizes(&doc.pages()[0].frame, "Very");
        assert!(!sizes.is_empty());
        assert!(
            sizes.iter().all(|size| (size - 4.0).abs() < 0.01),
            "{sizes:?}"
        );
    }

    #[test]
    fn sticker_with_a_price_element_but_no_price_fails_to_compile() {
        let engine = RenderEngine::warm_up("templates", "fonts").expect("warm_up should succeed");
        let name = discovered(&engine, "Product Sticker Label").name.clone();
        let mut sample = sticker_sample();
        sample["labels"][0]["priceCents"] = serde_json::Value::Null;

        let warned = engine.compile(
            &name,
            crate::modules::render::repository::typst::json_to_dict(sample),
        );
        assert!(warned.output.is_err());
    }

    #[test]
    fn compile_once_matches_the_shared_engine_for_the_same_input() {
        let engine = RenderEngine::warm_up("templates", "fonts").expect("warm_up should succeed");
        let name = discovered(&engine, "Product Sticker Label").name.clone();
        let rel = engine.template_rel_path(&name).unwrap().to_string();

        let warned = engine.compile_once(&rel, sticker_input());
        let doc = warned
            .output
            .expect("compile_once should compile the sticker");
        let pdf_bytes =
            typst_pdf::pdf(&doc, &Default::default()).expect("pdf export should succeed");
        assert!(pdf_bytes.starts_with(b"%PDF-"));
    }
}
