// Domain models and DTOs for the barcodes feature.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Supported 1D barcode symbologies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum BarcodeSymbology {
    #[default]
    Code128,
    Code39,
    Ean13,
    Ean8,
}

impl std::fmt::Display for BarcodeSymbology {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Code128 => write!(f, "code128"),
            Self::Code39 => write!(f, "code39"),
            Self::Ean13 => write!(f, "ean13"),
            Self::Ean8 => write!(f, "ean8"),
        }
    }
}

impl std::str::FromStr for BarcodeSymbology {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "code128" | "code_128" => Ok(Self::Code128),
            "code39" | "code_39" => Ok(Self::Code39),
            "ean13" | "ean_13" => Ok(Self::Ean13),
            "ean8" | "ean_8" => Ok(Self::Ean8),
            other => Err(format!("unsupported symbology: {other}")),
        }
    }
}

/// Request payload for generating a barcode.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GenerateBarcodeRequest {
    /// Content or string value to encode into the barcode.
    pub content: String,
    /// Barcode symbology (defaults to code128).
    #[serde(default)]
    pub symbology: BarcodeSymbology,
    /// Height in pixels/units (optional, defaults to 80).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

/// Response payload containing generated barcode SVG data.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BarcodeResponse {
    pub content: String,
    pub symbology: BarcodeSymbology,
    /// Rendered SVG vector string.
    pub svg: String,
}
