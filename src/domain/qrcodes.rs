// Domain models and DTOs for the QR codes feature.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// QR code error correction level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum QrEccLevel {
    L,
    #[default]
    M,
    Q,
    H,
}

impl std::fmt::Display for QrEccLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::L => write!(f, "L"),
            Self::M => write!(f, "M"),
            Self::Q => write!(f, "Q"),
            Self::H => write!(f, "H"),
        }
    }
}

impl std::str::FromStr for QrEccLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "L" => Ok(Self::L),
            "M" => Ok(Self::M),
            "Q" => Ok(Self::Q),
            "H" => Ok(Self::H),
            other => Err(format!("unknown ECC level: {other}")),
        }
    }
}

/// Request payload for generating a QR code.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GenerateQrCodeRequest {
    /// Text or URL payload to encode in the QR code.
    pub content: String,
    /// Error correction level: L (7%), M (15%), Q (25%), H (30%). Defaults to M.
    #[serde(default)]
    pub ecc: QrEccLevel,
}

/// Response payload containing generated QR code SVG data.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QrCodeResponse {
    pub content: String,
    pub ecc: QrEccLevel,
    /// Rendered SVG vector string.
    pub svg: String,
}
