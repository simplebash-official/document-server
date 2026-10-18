// `modules::packages::dry_run` — the offline check behind the
// `validate_package` binary: the same verdict as publishing, with no server.

use std::{io::Write, path::Path};

use document_server::modules::packages::dry_run;

fn zip(files: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in files {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(data.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

const MANIFEST: &str = r#"{"format":1,"type":"document","minAppVersion":null}"#;
const TYP: &str = "#import \"../../lib/money.typ\": format-money\n#import sys: inputs\n#set page(width: 100mm, height: 50mm)\n= #inputs.title #format-money(1500)\n";
const SCHEMA: &str = r#"{"title":"Dry Run Design","x-document-kind":"a4-invoice","type":"object","required":["title"],"properties":{"title":{"type":"string"}}}"#;
const SAMPLE: &str = r#"{"title":"Hello"}"#;

fn check(files: &[(&str, &str)]) -> Result<dry_run::DryRunReport, String> {
    dry_run::run(Path::new("templates"), Path::new("fonts"), &zip(files))
}

#[test]
fn a_valid_package_renders_its_sample_and_leaves_nothing_behind() {
    let report = check(&[
        ("manifest.json", MANIFEST),
        ("template.typ", TYP),
        ("schema.json", SCHEMA),
        ("sample.json", SAMPLE),
    ])
    .expect("package is valid");
    assert_eq!(report.title, "Dry Run Design");
    assert_eq!(report.kind, "a4-invoice");
    assert!(report.sample_pdf.starts_with(b"%PDF-"));
    let leftovers = std::fs::read_dir(std::env::temp_dir())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("validate_package_")
        })
        .count();
    assert_eq!(leftovers, 0, "the scratch folder is removed");
}

#[test]
fn the_same_rules_as_publishing_apply() {
    let base = [
        ("manifest.json", MANIFEST),
        ("schema.json", SCHEMA),
        ("sample.json", SAMPLE),
    ];
    let with_typ = |typ: &'static str| {
        let mut files = base.to_vec();
        files.push(("template.typ", typ));
        check(&files)
    };
    let err = with_typ("#import \"../lib/money.typ\": *\n= Hi\n").unwrap_err();
    assert!(err.contains("../../lib/"), "{err}");
    let err = with_typ("#let x = ( // unterminated\n").unwrap_err();
    assert!(err.contains("does not render"), "{err}");
    let err = check(&base).unwrap_err();
    assert!(err.contains("template.typ"), "{err}");
}

#[test]
fn identity_in_an_upload_is_refused() {
    let err = check(&[
        (
            "manifest.json",
            r#"{"format":1,"type":"document","name":"doc_temp_aaaaaaaaaaaaaaaa"}"#,
        ),
        ("template.typ", TYP),
        ("schema.json", SCHEMA),
        ("sample.json", SAMPLE),
    ])
    .unwrap_err();
    assert!(err.contains("server assigns"), "{err}");
}
