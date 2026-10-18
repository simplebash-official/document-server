// Business rules for template packages: validate an uploaded zip, give it an
// identity, prove it compiles, put its files on disk atomically and record the
// row. A published package is never modified afterwards.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use chrono::Utc;
use regex::Regex;
use sqlx::{SqlitePool, types::Json};

use crate::{
    clients::render::{PACKAGES_SUBDIR, RenderEngine, sha256_hex},
    core::{
        config::Config,
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::{generate_id, generate_template_name},
    },
    domain::{
        packages::{MANIFEST_FORMAT, Manifest, PackageInfo, PackagesResponse},
        templates::{TemplateScope, TemplateType},
    },
    modules::{
        packages::archive::{self, ParsedPackage},
        render,
        templates::{self, model::TemplateRow},
    },
};

const FAMILY_PREFIX: &str = prefixes::TEMPLATE_FAMILY;

fn invalid(message: impl Into<String>) -> AppError {
    AppError::unprocessable_entity(codes::PACKAGE_INVALID, message)
}

fn not_found() -> AppError {
    AppError::not_found_with_code("Package not found", codes::PACKAGE_NOT_FOUND)
}

fn conflict(message: &str) -> AppError {
    AppError::custom(
        axum::http::StatusCode::CONFLICT,
        codes::PACKAGE_CONFLICT,
        message,
    )
}

fn packages_dir(cfg: &Config) -> PathBuf {
    Path::new(&cfg.templates_dir).join(PACKAGES_SUBDIR)
}

fn is_name(value: &str) -> bool {
    Regex::new(r"^(doc|lbl)_temp_[A-Za-z0-9_-]{16}$")
        .expect("static regex")
        .is_match(value)
}

fn is_family(value: &str) -> bool {
    Regex::new(&format!(r"^{FAMILY_PREFIX}_[A-Za-z0-9_-]{{16}}$"))
        .expect("static regex")
        .is_match(value)
}

/// Rendering a package's sample must never reach the network, whatever the
/// server's image-fetch setting is.
fn offline(cfg: &Config) -> Config {
    Config {
        remote_image_fetch_enabled: false,
        ..cfg.clone()
    }
}

async fn info_from_row(db: &SqlitePool, row: TemplateRow) -> AppResult<PackageInfo> {
    let scope = row
        .scope()
        .map_err(|err| AppError::internal(format!("template row is corrupt: {err}")))?;
    let grants = if scope == TemplateScope::Private {
        templates::service::list_grants(db)
            .await?
            .into_iter()
            .filter(|(family, _)| *family == row.family)
            .map(|(_, tenant)| tenant)
            .collect()
    } else {
        Vec::new()
    };
    Ok(PackageInfo {
        template: templates::service::into_template(row)?,
        grants,
    })
}

pub(crate) async fn list(db: &SqlitePool) -> AppResult<PackagesResponse> {
    let mut grants: HashMap<String, Vec<String>> = HashMap::new();
    for (family, tenant) in templates::service::list_grants(db).await? {
        grants.entry(family).or_default().push(tenant);
    }
    let mut packages = Vec::new();
    for row in templates::service::list_all(db).await? {
        let scope = row
            .scope()
            .map_err(|err| AppError::internal(format!("template row is corrupt: {err}")))?;
        let family_grants = if scope == TemplateScope::Private {
            grants.get(&row.family).cloned().unwrap_or_default()
        } else {
            Vec::new()
        };
        packages.push(PackageInfo {
            template: templates::service::into_template(row)?,
            grants: family_grants,
        });
    }
    Ok(PackagesResponse { packages })
}

pub(crate) async fn get(db: &SqlitePool, name: &str) -> AppResult<PackageInfo> {
    let row = templates::service::get_by_name(db, name)
        .await
        .map_err(|_| not_found())?;
    info_from_row(db, row).await
}

/// Publishes `bytes` as the next version of `family`, or as version 1 of a
/// new family when `family` is `None`. The server assigns the identity.
pub(crate) async fn publish(
    db: &SqlitePool,
    render: &RenderEngine,
    cfg: &Config,
    bytes: &[u8],
    family: Option<&str>,
) -> AppResult<PackageInfo> {
    let parsed = archive::parse(bytes).map_err(invalid)?;
    if parsed.manifest.name.is_some()
        || parsed.manifest.family.is_some()
        || parsed.manifest.version.is_some()
        || parsed.manifest.created_at.is_some()
    {
        return Err(invalid(
            "manifest.json must not name name, family, version or createdAt: the server assigns them",
        ));
    }

    let (family, version) = match family {
        None => (generate_id(FAMILY_PREFIX), 1),
        Some(family) => {
            if !is_family(family) {
                return Err(invalid("family is not a valid family id"));
            }
            ensure_same_design(db, family, &parsed).await?;
            (
                family.to_string(),
                templates::service::next_version(db, family).await?,
            )
        }
    };
    if templates::service::title_used_by_other_family(db, &parsed.schema_info.title, &family)
        .await?
    {
        return Err(invalid(format!(
            "another design is already called '{}'",
            parsed.schema_info.title
        )));
    }

    let prefix = match parsed.manifest.r#type {
        TemplateType::Document => prefixes::TEMPLATE_NAME_DOCUMENT,
        TemplateType::Label => prefixes::TEMPLATE_NAME_LABEL,
    };
    let name = generate_template_name(prefix);
    let now = Utc::now();
    let manifest = Manifest {
        format: MANIFEST_FORMAT,
        r#type: parsed.manifest.r#type,
        min_app_version: parsed.manifest.min_app_version.clone(),
        name: Some(name.clone()),
        family: Some(family.clone()),
        version: Some(version),
        created_at: Some(now),
    };
    let archive_bytes = archive::build(&parsed, &manifest)
        .map_err(|err| AppError::internal(format!("could not build archive: {err}")))?;

    let row = store(db, render, cfg, &parsed, &archive_bytes, &manifest).await?;
    info_from_row(db, row).await
}

/// Installs an exported archive under the identity its manifest names
/// (replication to a desktop sidecar). Idempotent: the same bytes again are a
/// no-op, different bytes under the same name are a conflict. Grants the
/// package's family to `tenant_key`. Returns whether the package was new.
pub(crate) async fn install(
    db: &SqlitePool,
    render: &RenderEngine,
    cfg: &Config,
    name: &str,
    expected_sha256: &str,
    bytes: &[u8],
    tenant_key: &str,
) -> AppResult<(PackageInfo, bool)> {
    if !is_name(name) {
        return Err(invalid("name is not a valid template name"));
    }
    let sha256 = sha256_hex(bytes);
    if !sha256.eq_ignore_ascii_case(expected_sha256) {
        return Err(invalid("sha256 does not match the uploaded archive"));
    }

    let parsed = archive::parse(bytes).map_err(invalid)?;
    let manifest = &parsed.manifest;
    let (Some(manifest_name), Some(family), Some(version), Some(_)) = (
        manifest.name.as_deref(),
        manifest.family.as_deref(),
        manifest.version,
        manifest.created_at,
    ) else {
        return Err(invalid(
            "manifest.json must name name, family, version and createdAt",
        ));
    };
    if manifest_name != name {
        return Err(invalid("manifest name does not match the URL"));
    }
    if !is_family(family) || version < 1 {
        return Err(invalid("manifest family or version is invalid"));
    }

    if let Ok(existing) = templates::service::get_by_name(db, name).await {
        if existing.sha256 != sha256 {
            return Err(conflict(
                "A different package with this name is already installed",
            ));
        }
        templates::service::add_grant(db, family, tenant_key).await?;
        return Ok((info_from_row(db, existing).await?, false));
    }
    ensure_same_design(db, family, &parsed).await?;
    if templates::service::title_used_by_other_family(db, &parsed.schema_info.title, family).await?
    {
        return Err(conflict(
            "another design is already called this on this server",
        ));
    }

    let row = store(db, render, cfg, &parsed, bytes, &parsed.manifest.clone()).await?;
    templates::service::add_grant(db, family, tenant_key).await?;
    Ok((info_from_row(db, row).await?, true))
}

/// A new version must keep the design's kind and type, and may only extend a
/// private family.
async fn ensure_same_design(
    db: &SqlitePool,
    family: &str,
    parsed: &ParsedPackage,
) -> AppResult<()> {
    let Some(head) = templates::service::family_head(db, family).await? else {
        return Ok(());
    };
    let scope = head
        .scope()
        .map_err(|err| AppError::internal(format!("template row is corrupt: {err}")))?;
    if scope != TemplateScope::Private {
        return Err(invalid("a bundled design cannot get new versions"));
    }
    if head.kind != parsed.schema_info.kind || head.r#type != parsed.manifest.r#type.to_string() {
        return Err(invalid(format!(
            "a new version must stay a {} {}",
            head.r#type, head.kind
        )));
    }
    Ok(())
}

/// Writes the package folder (via a temp folder, so a half-written package is
/// never visible), proves the sample compiles, then records the row.
async fn store(
    db: &SqlitePool,
    render: &RenderEngine,
    cfg: &Config,
    parsed: &ParsedPackage,
    archive_bytes: &[u8],
    manifest: &Manifest,
) -> AppResult<TemplateRow> {
    let name = manifest.name.clone().expect("identity is set before store");
    let family = manifest
        .family
        .clone()
        .expect("identity is set before store");
    let version = manifest.version.expect("identity is set before store");
    let created_at = manifest.created_at.expect("identity is set before store");

    let root = packages_dir(cfg);
    let staging_name = format!(".tmp_{}", nanoid::nanoid!(12));
    let staging = root.join(&staging_name);
    let final_dir = root.join(&name);

    let write = async {
        tokio::fs::create_dir_all(staging.join("assets")).await?;
        tokio::fs::write(staging.join("template.typ"), &parsed.source).await?;
        tokio::fs::write(
            staging.join("schema.json"),
            serde_json::to_vec_pretty(&parsed.schema)?,
        )
        .await?;
        tokio::fs::write(
            staging.join("sample.json"),
            serde_json::to_vec_pretty(&parsed.sample)?,
        )
        .await?;
        for (asset, data) in &parsed.assets {
            tokio::fs::write(staging.join("assets").join(asset), data).await?;
        }
        tokio::fs::write(staging.join("package.zip"), archive_bytes).await?;
        Ok::<(), std::io::Error>(())
    };
    if let Err(err) = write.await {
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err(AppError::internal(format!(
            "could not write package files: {err}"
        )));
    }

    let row = TemplateRow {
        key: generate_id(prefixes::TEMPLATE),
        name: name.clone(),
        family,
        version,
        r#type: manifest.r#type.to_string(),
        kind: parsed.schema_info.kind.clone(),
        description: parsed.schema_info.title.clone(),
        data_schema: Json(parsed.schema.clone()),
        sample_data: Json(parsed.sample.clone()),
        status: "published".to_string(),
        scope: "private".to_string(),
        file_path: format!("{PACKAGES_SUBDIR}/{name}/template.typ"),
        sha256: sha256_hex(archive_bytes),
        min_app_version: manifest.min_app_version.clone(),
        created_at,
        updated_at: created_at,
    };

    // Compile from the staging folder: it sits beside the final one, so the
    // `../../lib/` imports resolve identically.
    let smoke = TemplateRow {
        file_path: format!("{PACKAGES_SUBDIR}/{staging_name}/template.typ"),
        ..row.clone()
    };
    if let Err(err) =
        render::service::compile_pdf(render, &offline(cfg), &smoke, parsed.sample.clone()).await
    {
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err(invalid(format!(
            "the template does not render its sample: {err}"
        )));
    }

    if let Err(err) = tokio::fs::rename(&staging, &final_dir).await {
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err(if final_dir.exists() {
            conflict("A package with this name already exists")
        } else {
            AppError::internal(format!("could not place package files: {err}"))
        });
    }

    if let Err(err) = templates::service::insert_private(db, &row).await {
        let _ = tokio::fs::remove_dir_all(&final_dir).await;
        return Err(err);
    }
    Ok(row)
}

/// The stored archive of a private package the tenant may use.
pub(crate) async fn archive_bytes(
    db: &SqlitePool,
    cfg: &Config,
    tenant_key: &str,
    name: &str,
) -> AppResult<Vec<u8>> {
    let row = templates::service::ensure_usable_by(db, tenant_key, name)
        .await
        .map_err(|_| not_found())?;
    if row
        .scope()
        .map_err(|e| AppError::internal(format!("template row is corrupt: {e}")))?
        != TemplateScope::Private
    {
        return Err(not_found());
    }
    tokio::fs::read(packages_dir(cfg).join(&row.name).join("package.zip"))
        .await
        .map_err(|err| AppError::internal(format!("could not read package archive: {err}")))
}

/// Renders the package's own sample — for the admin console's preview. Not
/// recorded as a document.
pub(crate) async fn preview(
    db: &SqlitePool,
    render: &RenderEngine,
    cfg: &Config,
    name: &str,
) -> AppResult<Vec<u8>> {
    let row = templates::service::get_by_name(db, name)
        .await
        .map_err(|_| not_found())?;
    let sample = row.sample_data.0.clone();
    render::service::compile_pdf(render, &offline(cfg), &row, sample).await
}

pub(crate) async fn set_grants(
    db: &SqlitePool,
    family: &str,
    tenant_keys: &[String],
) -> AppResult<()> {
    require_private_family(db, family).await?;
    let mut unique: Vec<String> = Vec::new();
    for key in tenant_keys {
        if key.is_empty()
            || key.len() > 128
            || !key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(AppError::validation(format!(
                "'{key}' is not a valid tenant key"
            )));
        }
        if !unique.contains(key) {
            unique.push(key.clone());
        }
    }
    templates::service::replace_grants(db, family, &unique).await
}

pub(crate) async fn retire(db: &SqlitePool, family: &str) -> AppResult<u64> {
    require_private_family(db, family).await?;
    templates::service::retire_family(db, family).await
}

async fn require_private_family(db: &SqlitePool, family: &str) -> AppResult<()> {
    let head = templates::service::family_head(db, family)
        .await?
        .ok_or_else(not_found)?;
    if head
        .scope()
        .map_err(|e| AppError::internal(format!("template row is corrupt: {e}")))?
        != TemplateScope::Private
    {
        return Err(AppError::validation(
            "a bundled design is available to everyone and cannot be granted or retired",
        ));
    }
    Ok(())
}
