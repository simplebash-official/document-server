use document_server::domain::qrcodes::{GenerateQrCodeRequest, QrEccLevel};
use document_server::modules::qrcodes::service::generate_qr_code;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let content = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "https://www.simplebash.com".to_string());

    let default_out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../typst-documents/invoices/invoice-1/simplebash_qr.svg");

    let output_path = std::env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or(default_out);

    let req = GenerateQrCodeRequest {
        content: content.clone(),
        ecc: QrEccLevel::M,
    };

    let res = generate_qr_code(&req)?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output_path, res.svg)?;
    println!(
        "Successfully generated QR code for '{}' at {:?}",
        content, output_path
    );
    Ok(())
}
