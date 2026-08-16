// QR code generation service logic. Encodes strings into 2D QR codes and outputs SVG.

use qrcode::{EcLevel, QrCode, render::svg};

use crate::{
    core::{
        constants::codes,
        error::{AppError, AppResult},
    },
    domain::qrcodes::{GenerateQrCodeRequest, QrCodeResponse, QrEccLevel},
};

/// Generates an SVG QR code for the given request payload.
pub fn generate_qr_code(req: &GenerateQrCodeRequest) -> AppResult<QrCodeResponse> {
    if req.content.trim().is_empty() {
        return Err(AppError::validation_with_code(
            "QR code content cannot be empty",
            codes::VALIDATION_ERROR,
        ));
    }

    let ecc = match req.ecc {
        QrEccLevel::L => EcLevel::L,
        QrEccLevel::M => EcLevel::M,
        QrEccLevel::Q => EcLevel::Q,
        QrEccLevel::H => EcLevel::H,
    };

    let code = QrCode::with_error_correction_level(&req.content, ecc).map_err(|err| {
        AppError::validation_with_code(
            format!("Failed to encode QR code: {err}"),
            codes::QR_GENERATION_FAILED,
        )
    })?;

    let svg_string = code.render::<svg::Color>().build();

    Ok(QrCodeResponse {
        content: req.content.clone(),
        ecc: req.ecc,
        svg: svg_string,
    })
}
