// Offline check of a package zip, for authors: runs exactly the validation
// `POST /api/packages` runs (archive rules, schema and options rules, import
// lint) and renders the package's sample on a throwaway engine — with no
// database, no server and nothing written outside a temp folder. Used by the
// `validate_package` binary; the shop-owner's coding agents call it through
// `document-templates/packages/tools/pkg.py check`.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{
    clients::render::{PACKAGES_SUBDIR, RenderEngine},
    core::config::Config,
    domain::templates::TemplateOption,
    modules::{packages::archive, render, templates::model::TemplateRow},
};

/// What a package would be published as, plus its rendered sample.
#[derive(Debug)]
pub struct DryRunReport {
    pub title: String,
    pub kind: String,
    pub r#type: String,
    pub options: Vec<TemplateOption>,
    pub sample_pdf: Vec<u8>,
}

/// Summary for people (printed as JSON by the binary).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunSummary<'a> {
    pub title: &'a str,
    pub kind: &'a str,
    pub r#type: &'a str,
    pub options: &'a [TemplateOption],
    pub sample_pdf_bytes: usize,
}

impl DryRunReport {
    pub fn summary(&self) -> DryRunSummary<'_> {
        DryRunSummary {
            title: &self.title,
            kind: &self.kind,
            r#type: &self.r#type,
            options: &self.options,
            sample_pdf_bytes: self.sample_pdf.len(),
        }
    }
}

/// Makes the shared template folders visible under `root` so a package staged
/// under `root/packages/` resolves `../../lib/…` exactly as on the server:
/// symlinks where the platform has them, copies elsewhere.
fn link_shared(templates_dir: &Path, root: &Path) -> Result<(), String> {
    let source_root = templates_dir.canonicalize().map_err(|err| {
        format!(
            "cannot read templates dir {}: {err}",
            templates_dir.display()
        )
    })?;
    for shared in ["documents", "labels", "lib"] {
        let (target, link) = (source_root.join(shared), root.join(shared));
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link)
            .map_err(|err| format!("cannot link {shared}: {err}"))?;
        #[cfg(not(unix))]
        copy_dir(&target, &link).map_err(|err| format!("cannot copy {shared}: {err}"))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Validates `bytes` as a publishable package and renders its sample.
/// `templates_dir` is the document-server's bundled `templates/` folder (it
/// supplies `lib/`), `fonts_dir` its `fonts/`. Errors are meant for the author.
pub fn run(templates_dir: &Path, fonts_dir: &Path, bytes: &[u8]) -> Result<DryRunReport, String> {
    let parsed = archive::parse(bytes)?;
    if parsed.manifest.name.is_some()
        || parsed.manifest.family.is_some()
        || parsed.manifest.version.is_some()
        || parsed.manifest.created_at.is_some()
    {
        return Err(
            "manifest.json must not name name, family, version or createdAt: the server assigns them"
                .to_string(),
        );
    }

    let scratch: PathBuf =
        std::env::temp_dir().join(format!("validate_package_{}", nanoid::nanoid!(10)));
    let result = (|| {
        let stage = scratch.join(PACKAGES_SUBDIR).join("stage");
        std::fs::create_dir_all(stage.join("assets")).map_err(|e| e.to_string())?;
        link_shared(templates_dir, &scratch)?;
        std::fs::write(stage.join("template.typ"), &parsed.source).map_err(|e| e.to_string())?;
        for (name, data) in &parsed.assets {
            std::fs::write(stage.join("assets").join(name), data).map_err(|e| e.to_string())?;
        }

        let engine = RenderEngine::warm_up(&scratch, fonts_dir).map_err(|e| e.to_string())?;
        let row = TemplateRow {
            key: "tpl_dryrun".to_string(),
            name: "doc_temp_dryrundryrundry".to_string(),
            family: "fam_dryrundryrundry".to_string(),
            version: 1,
            r#type: parsed.manifest.r#type.to_string(),
            kind: parsed.schema_info.kind.clone(),
            description: parsed.schema_info.title.clone(),
            data_schema: sqlx::types::Json(parsed.schema.clone()),
            sample_data: sqlx::types::Json(parsed.sample.clone()),
            status: "published".to_string(),
            scope: "private".to_string(),
            file_path: format!("{PACKAGES_SUBDIR}/stage/template.typ"),
            sha256: String::new(),
            min_app_version: parsed.manifest.min_app_version.clone(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let cfg = offline_config(&scratch);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .map_err(|e| e.to_string())?;
        let pdf = runtime
            .block_on(render::service::compile_pdf(
                &engine,
                &cfg,
                &row,
                parsed.sample.clone(),
            ))
            .map_err(|e| format!("the template does not render its sample: {e}"))?;

        Ok(DryRunReport {
            title: parsed.schema_info.title.clone(),
            kind: parsed.schema_info.kind.clone(),
            r#type: parsed.manifest.r#type.to_string(),
            options: parsed.schema_info.options.clone(),
            sample_pdf: pdf,
        })
    })();
    let _ = std::fs::remove_dir_all(&scratch);
    result
}

/// A config that never reaches the network and stages files under `root`.
fn offline_config(root: &Path) -> Config {
    Config {
        database_url: String::new(),
        port: 0,
        bind_addr: "127.0.0.1".to_string(),
        templates_dir: root.display().to_string(),
        fonts_dir: String::new(),
        max_render_body_bytes: 0,
        max_package_bytes: 0,
        internal_api_key: String::new(),
        remote_image_max_bytes: 0,
        remote_image_timeout_secs: 1,
        remote_image_fetch_enabled: false,
        allow_private_ip_images: false,
        app_env: "development".to_string(),
    }
}
