// Barcode generation service logic. Encodes strings into 1D barcodes and outputs SVG.

use barcoders::generators::svg::SVG;
use barcoders::sym::code39::Code39;
use barcoders::sym::code128::Code128;
use barcoders::sym::ean8::EAN8;
use barcoders::sym::ean13::EAN13;

use crate::{
    core::{
        constants::codes,
        error::{AppError, AppResult},
    },
    domain::barcodes::{BarcodeResponse, BarcodeSymbology, GenerateBarcodeRequest},
};

/// Generates an SVG barcode for the given request payload.
pub fn generate_barcode(req: &GenerateBarcodeRequest) -> AppResult<BarcodeResponse> {
    if req.content.trim().is_empty() {
        return Err(AppError::validation_with_code(
            "barcode content cannot be empty",
            codes::VALIDATION_ERROR,
        ));
    }

    let height = req.height.unwrap_or(80);
    let svg_generator = SVG::new(height);

    let encoded_data: Vec<u8> = match req.symbology {
        BarcodeSymbology::Code128 => {
            let upper = req.content.to_ascii_uppercase();
            let formatted_content = if upper.starts_with('\u{00C0}') {
                upper
            } else {
                format!("\u{00C0}{}", upper)
            };
            let code = Code128::new(&formatted_content).map_err(|err| {
                AppError::validation_with_code(
                    format!("Invalid Code128 payload: {err:?}"),
                    codes::INVALID_BARCODE_FORMAT,
                )
            })?;
            code.encode()
        }
        BarcodeSymbology::Code39 => {
            let code = Code39::new(&req.content).map_err(|err| {
                AppError::validation_with_code(
                    format!("Invalid Code39 payload: {err:?}"),
                    codes::INVALID_BARCODE_FORMAT,
                )
            })?;
            code.encode()
        }
        BarcodeSymbology::Ean13 => {
            let code = EAN13::new(&req.content).map_err(|err| {
                AppError::validation_with_code(
                    format!("Invalid EAN13 payload: {err:?}"),
                    codes::INVALID_BARCODE_FORMAT,
                )
            })?;
            code.encode()
        }
        BarcodeSymbology::Ean8 => {
            let code = EAN8::new(&req.content).map_err(|err| {
                AppError::validation_with_code(
                    format!("Invalid EAN8 payload: {err:?}"),
                    codes::INVALID_BARCODE_FORMAT,
                )
            })?;
            code.encode()
        }
    };

    let svg_string = svg_generator.generate(&encoded_data).map_err(|err| {
        AppError::validation_with_code(
            format!("SVG generation error: {err:?}"),
            codes::BARCODE_GENERATION_FAILED,
        )
    })?;

    Ok(BarcodeResponse {
        content: req.content.clone(),
        symbology: req.symbology,
        svg: svg_string,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code128_symbology() {
        let req = GenerateBarcodeRequest {
            content: "INV-123456".to_string(),
            symbology: BarcodeSymbology::Code128,
            height: Some(60),
        };
        let res = generate_barcode(&req).expect("code128 should generate successfully");
        assert!(res.svg.contains("<svg"));
    }
}
