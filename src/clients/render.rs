// The embedded Typst compiler, built once at startup and shared read-only
// via `AppState` — the same "connect/build once, hand out a shared handle"
// shape as `clients::sqlite::connect`. Per-request compile orchestration
// (JSON -> Typst input, error translation) lives in
// `modules::render::repository::typst`, not here — this module only owns
// the engine's lifecycle: reading fonts off disk once, discovering the
// bundled templates, and building the `TypstEngine`.
//
// Only the bundled templates (`documents/`, `labels/`) are known to the
// engine. Private packages live in `packages/<name>/`, are looked up by the
// `file_path` stored in their `templates` row, and always compile through
// `compile_once`, so published files never enter a shared cache and no engine
// rebuild is ever needed to add one.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use typst::foundations::Dict;
use typst_as_lib::{TypstEngine, TypstTemplateCollection};
use typst_layout::PagedDocument;

use crate::domain::templates::{TemplateType, validate_template_schema};

/// Subdirectory of `TEMPLATES_DIR` that holds private packages. Never scanned
/// as bundled templates.
pub const PACKAGES_SUBDIR: &str = "packages";

/// Filename prefix for a per-render temp image (a downloaded `logoUrl`)
/// staged next to a template. Dot-prefixed so the scan skips it and so
/// `warm_up`'s sweep can recognise and delete any left by a crash.
pub const REMOTE_IMAGE_PREFIX: &str = ".rimg_";

/// Why `RenderEngine::warm_up` failed. A misconfigured `TEMPLATES_DIR`/
/// `FONTS_DIR` or a broken bundled template is a hard startup error — same
/// fail-fast posture as `Config::from_env()` — an operator should find out at
/// boot, not on the first render request.
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
    #[error("bundled template {name} is invalid: {reason}")]
    BundledTemplateInvalid { name: String, reason: String },
    #[error(
        "document kind '{kind}' must have exactly one bundled x-default template, found {count}"
    )]
    DefaultTemplateCount { kind: String, count: usize },
}

/// A template shipped in `TEMPLATES_DIR/{documents,labels}`, read and
/// validated at startup. `schema.json` and sample sidecars are mandatory.
#[derive(Debug, Clone)]
pub struct BundledTemplate {
    pub name: String,
    pub file_path: String,
    pub template_type: TemplateType,
    pub kind: String,
    pub title: String,
    pub is_default: bool,
    pub data_schema: serde_json::Value,
    pub sample_data: serde_json::Value,
    /// Hex SHA-256 of the `.typ` source.
    pub sha256: String,
}

pub struct RenderEngine {
    engine: TypstEngine<TypstTemplateCollection>,
    bundled: Vec<BundledTemplate>,
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
    /// memory (`.fonts()` takes owned bytes — a genuine preload), discovers
    /// and validates the bundled templates, points the engine at
    /// `templates_dir` via `with_file_system_resolver`, then runs one
    /// best-effort trial compile per bundled template purely to catch outright
    /// breakage (bad syntax, a missing asset, a bad font family) before the
    /// first real request.
    pub fn warm_up(
        templates_dir: impl AsRef<Path>,
        fonts_dir: impl AsRef<Path>,
    ) -> Result<Self, RenderEngineError> {
        let templates_dir = templates_dir.as_ref();
        let fonts_dir = fonts_dir.as_ref();

        let fonts = Arc::new(read_font_files(fonts_dir)?);
        let bundled = scan_bundled(templates_dir)?;
        check_one_default_per_kind(&bundled)?;

        // Delete any per-render temp images a previous process left behind
        // (crash between staging and cleanup). Best-effort — a failure here
        // must not block startup.
        sweep_remote_images(templates_dir);

        let engine = TypstEngine::builder()
            .fonts((*fonts).clone())
            .with_file_system_resolver(templates_dir.to_path_buf())
            .build();

        for tpl in &bundled {
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
            bundled,
            templates_dir: templates_dir.to_path_buf(),
            fonts,
        })
    }

    pub fn bundled(&self) -> &[BundledTemplate] {
        &self.bundled
    }

    /// Compiles a bundled template through the shared engine. Only valid for
    /// `file_path`s that were present at `warm_up`.
    pub(crate) fn compile_bundled(
        &self,
        file_path: &str,
        input: Dict,
    ) -> typst::diag::Warned<Result<PagedDocument, typst_as_lib::TypstAsLibError>> {
        self.engine.compile_with_input(file_path, input)
    }

    /// Compiles `main_rel_path` through a **freshly built, single-use**
    /// engine rooted at this engine's templates dir. Used for every private
    /// package and for any render that staged a downloaded image on disk: the
    /// throwaway engine — and the unbounded file cache
    /// `with_file_system_resolver` gives it — is dropped when this returns, so
    /// neither a package nor a staged image ever lands in the shared engine's
    /// cache (which would keep it in memory for the process's life). Fonts are
    /// reused from the shared engine's preloaded bytes.
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
}

fn check_one_default_per_kind(bundled: &[BundledTemplate]) -> Result<(), RenderEngineError> {
    let mut kinds: Vec<&str> = bundled.iter().map(|t| t.kind.as_str()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    for kind in kinds {
        let count = bundled
            .iter()
            .filter(|t| t.kind == kind && t.is_default)
            .count();
        if count != 1 {
            return Err(RenderEngineError::DefaultTemplateCount {
                kind: kind.to_string(),
                count,
            });
        }
    }
    Ok(())
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

/// Scans `TEMPLATES_DIR/documents` and `TEMPLATES_DIR/labels` for bundled
/// `.typ` templates. Each needs its `<stem>.schema.json` and `<stem>.json`
/// sidecars; anything unreadable or invalid fails startup.
fn scan_bundled(dir: &Path) -> Result<Vec<BundledTemplate>, RenderEngineError> {
    let mut templates = Vec::new();

    for (subdir, template_type) in [
        ("documents", TemplateType::Document),
        ("labels", TemplateType::Label),
    ] {
        let sub_path = dir.join(subdir);
        let entries = std::fs::read_dir(&sub_path).map_err(|source| {
            RenderEngineError::TemplatesDirUnreadable {
                path: sub_path.clone(),
                source,
            }
        })?;

        for entry in entries {
            let path = entry
                .map_err(|source| RenderEngineError::TemplatesDirUnreadable {
                    path: sub_path.clone(),
                    source,
                })?
                .path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("typ") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if stem.starts_with('.') {
                continue;
            }
            templates.push(read_bundled(&path, stem, subdir, template_type)?);
        }
    }

    templates.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(templates)
}

fn read_bundled(
    typ_path: &Path,
    stem: &str,
    subdir: &str,
    template_type: TemplateType,
) -> Result<BundledTemplate, RenderEngineError> {
    let invalid = |reason: String| RenderEngineError::BundledTemplateInvalid {
        name: stem.to_string(),
        reason,
    };

    let source =
        std::fs::read(typ_path).map_err(|err| invalid(format!("cannot read .typ: {err}")))?;
    let read_json = |ext: &str| -> Result<serde_json::Value, RenderEngineError> {
        let path = typ_path.with_extension(ext);
        let text = std::fs::read_to_string(&path)
            .map_err(|err| invalid(format!("cannot read {}: {err}", path.display())))?;
        serde_json::from_str(&text)
            .map_err(|err| invalid(format!("{} is not valid JSON: {err}", path.display())))
    };
    let data_schema = read_json("schema.json")?;
    let sample_data = read_json("json")?;
    let info = validate_template_schema(&data_schema).map_err(invalid)?;

    Ok(BundledTemplate {
        name: stem.to_string(),
        file_path: format!("{subdir}/{stem}.typ"),
        template_type,
        kind: info.kind,
        title: info.title,
        is_default: info.is_default,
        data_schema,
        sample_data,
        sha256: sha256_hex(&source),
    })
}

/// Lowercase hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Best-effort delete of every `REMOTE_IMAGE_PREFIX`-named temp image under
/// `dir` and its immediate subdirectories (`documents/`, `labels/`, and each `packages/<name>/`). Runs
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
    if let Ok(entries) = std::fs::read_dir(dir.join(PACKAGES_SUBDIR)) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                sweep_one(&entry.path());
            }
        }
    }
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

    /// Templates on disk have opaque generated names; a test that wants
    /// "the thermal receipt" resolves it through the human label carried in
    /// the schema `title`.
    fn discovered<'a>(engine: &'a RenderEngine, title: &str) -> &'a BundledTemplate {
        engine
            .bundled()
            .iter()
            .find(|t| t.title == title)
            .unwrap_or_else(|| panic!("no bundled template titled {title:?}"))
    }

    #[test]
    fn warm_up_and_compile_thermal_receipt_produces_a_pdf() {
        let engine = RenderEngine::warm_up("templates", "fonts").expect("warm_up should succeed");
        let template = discovered(&engine, "Thermal Receipt");
        assert_eq!(template.template_type, TemplateType::Document);
        let template_path = template.file_path.clone();

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

        let warned = engine.compile_bundled(&template_path, input);
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
        let template_path = template.file_path.clone();

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

        let warned = engine.compile_bundled(&template_path, input);
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
        let template_path = template.file_path.clone();
        // The vendored library files must never be mistaken for templates.
        assert!(!engine.bundled().iter().any(|t| t.name == "lib"));

        let warned = engine.compile_bundled(&template_path, sticker_input());
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
        let name = discovered(&engine, "Product Sticker Label")
            .file_path
            .clone();
        let mut sample = sticker_sample();
        sample["widthMm"] = serde_json::json!(58);
        sample["heightMm"] = serde_json::json!(40);
        sample["elements"][1]["widthMm"] = serde_json::json!(54);

        let doc = engine
            .compile_bundled(
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
        let template = discovered(&engine, "Product Sticker Label")
            .file_path
            .clone();
        let mut sample = sticker_sample();
        sample["labels"][0]["name"] = serde_json::json!(name);
        engine
            .compile_bundled(
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
        let name = discovered(&engine, "Product Sticker Label")
            .file_path
            .clone();
        let mut sample = sticker_sample();
        sample["labels"][0]["priceCents"] = serde_json::Value::Null;

        let warned = engine.compile_bundled(
            &name,
            crate::modules::render::repository::typst::json_to_dict(sample),
        );
        assert!(warned.output.is_err());
    }

    #[test]
    fn compile_once_matches_the_shared_engine_for_the_same_input() {
        let engine = RenderEngine::warm_up("templates", "fonts").expect("warm_up should succeed");
        let rel = discovered(&engine, "Product Sticker Label")
            .file_path
            .clone();

        let warned = engine.compile_once(&rel, sticker_input());
        let doc = warned
            .output
            .expect("compile_once should compile the sticker");
        let pdf_bytes =
            typst_pdf::pdf(&doc, &Default::default()).expect("pdf export should succeed");
        assert!(pdf_bytes.starts_with(b"%PDF-"));
    }
}
