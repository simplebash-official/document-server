// The package zip format.
//
//   manifest.json   format, type, minAppVersion (+ identity in stored archives)
//   template.typ    the Typst source
//   schema.json     JSON Schema input contract (+ x-document-kind, x-options)
//   sample.json     a worked example payload
//   assets/<file>   optional png/jpg/jpeg/svg images, one level deep
//
// Nothing here touches the database or the render engine: it turns bytes into
// a validated `ParsedPackage` and a `ParsedPackage` back into canonical bytes.

use std::io::{Cursor, Read, Write};

use regex::Regex;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::domain::{
    packages::{MANIFEST_FORMAT, Manifest},
    templates::{SchemaInfo, validate_template_schema},
};

const MAX_ENTRIES: usize = 200;
/// Hard cap on a single decompressed file, so a small zip cannot expand into
/// gigabytes of memory.
const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
const ASSET_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "svg"];

pub(crate) struct ParsedPackage {
    pub manifest: Manifest,
    pub source: Vec<u8>,
    pub schema: serde_json::Value,
    pub schema_info: SchemaInfo,
    pub sample: serde_json::Value,
    /// `(file name inside assets/, bytes)`, sorted by name.
    pub assets: Vec<(String, Vec<u8>)>,
}

/// Reads and validates a package zip. Every failure message is meant for the
/// person publishing the package.
pub(crate) fn parse(bytes: &[u8]) -> Result<ParsedPackage, String> {
    let mut zip =
        ZipArchive::new(Cursor::new(bytes)).map_err(|err| format!("not a valid zip: {err}"))?;
    if zip.len() > MAX_ENTRIES {
        return Err(format!("the zip has more than {MAX_ENTRIES} entries"));
    }

    let mut manifest = None;
    let mut source = None;
    let mut schema = None;
    let mut sample = None;
    let mut assets: Vec<(String, Vec<u8>)> = Vec::new();

    for index in 0..zip.len() {
        let mut file = zip
            .by_index(index)
            .map_err(|err| format!("cannot read zip entry {index}: {err}"))?;
        if file.is_dir() {
            continue;
        }
        if file
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(format!("'{}' is a symbolic link", file.name()));
        }
        let name = file
            .enclosed_name()
            .ok_or_else(|| format!("'{}' has an unsafe path", file.name()))?
            .to_string_lossy()
            .replace('\\', "/");
        if name.split('/').any(|part| part.starts_with('.')) {
            return Err(format!("'{name}' is a hidden file"));
        }

        let mut data = Vec::new();
        file.by_ref()
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut data)
            .map_err(|err| format!("cannot read '{name}': {err}"))?;
        if data.len() as u64 > MAX_FILE_BYTES {
            return Err(format!(
                "'{name}' is larger than {} MiB",
                MAX_FILE_BYTES / 1024 / 1024
            ));
        }

        match name.as_str() {
            "manifest.json" => manifest = Some(data),
            "template.typ" => source = Some(data),
            "schema.json" => schema = Some(data),
            "sample.json" => sample = Some(data),
            other => {
                let file_name = other
                    .strip_prefix("assets/")
                    .filter(|rest| !rest.is_empty() && !rest.contains('/'))
                    .ok_or_else(|| format!("unexpected file '{other}'"))?;
                let extension = file_name
                    .rsplit_once('.')
                    .map(|(_, ext)| ext.to_ascii_lowercase());
                if !extension.is_some_and(|ext| ASSET_EXTENSIONS.contains(&ext.as_str())) {
                    return Err(format!(
                        "asset '{file_name}' must be one of: {}",
                        ASSET_EXTENSIONS.join(", ")
                    ));
                }
                assets.push((file_name.to_string(), data));
            }
        }
    }

    let manifest: Manifest = serde_json::from_slice(&manifest.ok_or("manifest.json is missing")?)
        .map_err(|err| format!("manifest.json is invalid: {err}"))?;
    if manifest.format != MANIFEST_FORMAT {
        return Err(format!("manifest format must be {MANIFEST_FORMAT}"));
    }
    if let Some(version) = &manifest.min_app_version
        && !is_semver(version)
    {
        return Err("minAppVersion must look like 1.22.0".to_string());
    }

    let source = source.ok_or("template.typ is missing")?;
    let source_text = std::str::from_utf8(&source).map_err(|_| "template.typ is not UTF-8 text")?;
    let schema: serde_json::Value =
        serde_json::from_slice(&schema.ok_or("schema.json is missing")?)
            .map_err(|err| format!("schema.json is not valid JSON: {err}"))?;
    let sample: serde_json::Value =
        serde_json::from_slice(&sample.ok_or("sample.json is missing")?)
            .map_err(|err| format!("sample.json is not valid JSON: {err}"))?;
    let schema_info = validate_template_schema(&schema)?;
    if schema_info.is_default {
        return Err("only bundled templates may set x-default".to_string());
    }

    assets.sort_by(|a, b| a.0.cmp(&b.0));
    check_paths(source_text, &assets)?;

    Ok(ParsedPackage {
        manifest,
        source,
        schema,
        schema_info,
        sample,
        assets,
    })
}

fn is_semver(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// Lints every file path the source names: imports may reach the shared
/// `../../lib/` only; everything else must be a file of this package. The
/// true boundary is the engine's file resolver rooted at `TEMPLATES_DIR`; this
/// keeps an honest package from depending on anything it does not ship.
fn check_paths(source: &str, assets: &[(String, Vec<u8>)]) -> Result<(), String> {
    // Comments are not code: a commented-out `image("…")` must not fail the lint. A `//` only starts
    // a comment at the start of a line or after whitespace, so `"https://…"` strings are left alone.
    let comments = Regex::new(r"(?s)/\*.*?\*/|(?m)(^|\s)//.*$").expect("static regex");
    let source = comments.replace_all(source, "$1");
    let pattern = Regex::new(
        r#"(?:#import|#include)\s+"([^"]+)"|\b(?:image|read|json|csv|xml|yaml|toml|cbor|bibliography)\(\s*"([^"]+)""#,
    )
    .expect("static regex");
    for capture in pattern.captures_iter(&source) {
        let path = capture
            .get(1)
            .or_else(|| capture.get(2))
            .map_or("", |m| m.as_str());
        if path.starts_with('@') {
            return Err(format!("'{path}': Typst packages are not available"));
        }
        if path.starts_with('/') || path.contains('\\') {
            return Err(format!("'{path}': use a relative path"));
        }
        if let Some(lib_path) = path.strip_prefix("../../lib/") {
            if lib_path.is_empty() || lib_path.contains("..") {
                return Err(format!("'{path}' is not a valid library path"));
            }
            continue;
        }
        if path.split('/').any(|part| part == "..") {
            return Err(format!(
                "'{path}': only ../../lib/ may be reached outside the package"
            ));
        }
        let local = path.strip_prefix("assets/");
        let is_asset = local.is_some_and(|name| assets.iter().any(|(n, _)| n == name));
        if !is_asset {
            return Err(format!("'{path}' is not a file of this package"));
        }
    }
    Ok(())
}

/// Builds the canonical archive for `package` with `manifest` (which carries
/// the identity). Entries are ordered and carry a fixed timestamp, so the same
/// content always yields the same bytes — and the same sha256.
pub(crate) fn build(package: &ParsedPackage, manifest: &Manifest) -> Result<Vec<u8>, String> {
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default());
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let mut put = |name: &str, data: &[u8]| -> Result<(), String> {
        writer
            .start_file(name, options)
            .map_err(|err| err.to_string())?;
        writer.write_all(data).map_err(|err| err.to_string())
    };
    put(
        "manifest.json",
        &serde_json::to_vec_pretty(manifest).map_err(|e| e.to_string())?,
    )?;
    put("template.typ", &package.source)?;
    put(
        "schema.json",
        &serde_json::to_vec_pretty(&package.schema).map_err(|e| e.to_string())?,
    )?;
    put(
        "sample.json",
        &serde_json::to_vec_pretty(&package.sample).map_err(|e| e.to_string())?,
    )?;
    for (name, data) in &package.assets {
        put(&format!("assets/{name}"), data)?;
    }
    Ok(writer.finish().map_err(|err| err.to_string())?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libraries_may_be_imported_from_the_shared_lib_only() {
        assert!(check_paths(r#"#import "../../lib/money.typ": m"#, &[]).is_ok());
        assert!(check_paths(r#"#import "../lib/money.typ": m"#, &[]).is_err());
        assert!(check_paths(r#"#import "../../lib/../packages/x/schema.json": m"#, &[]).is_err());
        assert!(check_paths(r#"#import "@preview/cetz:0.2.0""#, &[]).is_err());
        assert!(check_paths(r#"#import "/etc/passwd""#, &[]).is_err());
    }

    #[test]
    fn images_must_be_shipped_assets() {
        let assets = vec![("sign.png".to_string(), vec![1])];
        assert!(check_paths(r#"#image("assets/sign.png")"#, &assets).is_ok());
        assert!(check_paths(r#"#image("assets/other.png")"#, &assets).is_err());
        assert!(check_paths(r#"#image("../other/x.png")"#, &assets).is_err());
    }

    #[test]
    fn commented_out_paths_are_not_checked() {
        assert!(check_paths("// #image(\"assets/missing.png\")\n= Hi", &[]).is_ok());
        assert!(check_paths("/* #import \"../lib/x.typ\" */\n= Hi", &[]).is_ok());
        assert!(check_paths("#import \"../lib/x.typ\" // not a comment trick", &[]).is_err());
    }

    #[test]
    fn semver_is_three_numeric_parts() {
        assert!(is_semver("1.22.0"));
        assert!(!is_semver("1.22"));
        assert!(!is_semver("1.x.0"));
    }
}
