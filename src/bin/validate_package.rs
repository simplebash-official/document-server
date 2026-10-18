// Checks a template package zip the way the server would, without a server:
//
//   cargo run --bin validate_package -- <package.zip> [--pdf <sample.pdf>]
//
// Runs the full publish validation and renders the package's sample. Exits 0
// and prints a JSON summary on success; exits 1 with the reason on stderr
// otherwise. `--pdf` also writes the rendered sample so the author can look
// at it. Reads the bundled `lib/` and fonts from `TEMPLATES_DIR` (default
// `templates`) and `FONTS_DIR` (default `fonts`), relative to this crate.

use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(zip_path) = args.next() else {
        eprintln!("usage: validate_package <package.zip> [--pdf <sample.pdf>]");
        std::process::exit(2);
    };
    let mut pdf_out: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pdf" => pdf_out = args.next().map(PathBuf::from),
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }

    let bytes = std::fs::read(&zip_path).unwrap_or_else(|err| {
        eprintln!("cannot read {zip_path}: {err}");
        std::process::exit(2);
    });
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let templates = std::env::var("TEMPLATES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| crate_dir.join("templates"));
    let fonts = std::env::var("FONTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| crate_dir.join("fonts"));

    match document_server::modules::packages::dry_run::run(&templates, &fonts, &bytes) {
        Ok(report) => {
            if let Some(path) = pdf_out
                && let Err(err) = std::fs::write(&path, &report.sample_pdf)
            {
                eprintln!("cannot write {}: {err}", path.display());
                std::process::exit(2);
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&report.summary()).expect("summary serializes")
            );
        }
        Err(reason) => {
            eprintln!("INVALID: {reason}");
            std::process::exit(1);
        }
    }
}
