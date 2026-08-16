// Unified Barcode and QR Code Library for Typst Templates.
// Provides both direct generation helpers and dynamic placeholder slots
// for document and label templates.

#import "barcode.typ": barcode
#import "qrcode.typ": qrcode

#let parse-dimension(val, fallback: 15mm) = {
  if val == none {
    fallback
  } else if type(val) == length {
    val
  } else if type(val) == int or type(val) == float {
    val * 1mm
  } else if type(val) == str {
    let s = val.trim()
    if s.ends-with("mm") {
      let num-str = s.slice(0, s.len() - 2).trim()
      float(num-str) * 1mm
    } else if s.ends-with("pt") {
      let num-str = s.slice(0, s.len() - 2).trim()
      float(num-str) * 1pt
    } else {
      fallback
    }
  } else {
    fallback
  }
}

/// Dynamic placeholder slot for document or label templates.
/// Inspects a `config` (which can be a string value or a dictionary of options)
/// and dynamically renders either a barcode or QR code where space is reserved.
///
/// Supported `config` dictionary options:
/// - `value` or `content`: The payload to encode (e.g. "INV-12345" or "https://pay.link/123").
/// - `type`: "barcode" | "qr" | "qrcode".
/// - `symbology`: "Code128" | "Code39" | "EAN13" | "EAN8" (for barcode).
/// - `height`: length or string (for barcode, e.g. 10mm or "10mm").
/// - `size`: length or string (for QR code, e.g. 15mm or "15mm").
/// - `showText`: boolean (for barcode human-readable text).
/// - `caption`: custom string text.
#let code-placeholder(
  config,
  default-type: "barcode",
  height: 10mm,
  size: 15mm,
  align-mode: center,
) = {
  if config == none {
    return
  }

  // Handle string directly
  if type(config) == str {
    if config.trim() == "" {
      return
    }
    align(align-mode)[
      #if default-type == "qrcode" or default-type == "qr" {
        qrcode(config, size: size)
      } else {
        barcode(config, height: height)
      }
    ]
    return
  }

  // Handle dictionary configuration
  if type(config) == dictionary {
    let raw-val = config.at("value", default: config.at("content", default: none))
    if raw-val == none or str(raw-val).trim() == "" {
      return
    }

    let code-type = config.at("type", default: default-type)
    align(align-mode)[
      #if code-type == "qrcode" or code-type == "qr" {
        let qr-size = parse-dimension(config.at("size", default: none), fallback: size)
        qrcode(raw-val, size: qr-size)
      } else {
        let symbology = config.at("symbology", default: "Code128")
        let bc-height = parse-dimension(config.at("height", default: none), fallback: height)
        let show-text = config.at("showText", default: true)
        let caption = config.at("caption", default: none)
        barcode(raw-val, symbology: symbology, height: bc-height, show-text: show-text, caption: caption)
      }
    ]
  }
}
