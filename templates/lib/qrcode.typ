// Dedicated QR Code Component for Typst templates.
// Wraps the vendored `zebra` package for scalable 2D vector QR codes.

#import "zebra/lib.typ" as zebra

/// Renders a 2D QR Code as sharp native vector curves.
/// - `data`: The string, URL, or data payload to encode in the QR code.
/// - `size`: Width and height of the QR code (defaults to 15mm).
/// - `width`: Specific width override (defaults to size).
/// - `quiet-zone`: Quiet zone margin around the code in module units (defaults to 1).
/// - `fill`: Fill color for the QR code modules (defaults to black).
/// - `background`: Background fill color (defaults to none).
#let qrcode(
  data,
  size: 15mm,
  width: auto,
  quiet-zone: 1,
  fill: black,
  background: none,
) = {
  if data == none or str(data).trim() == "" {
    return
  }

  let final-width = if width != auto { width } else { size }

  zebra.qrcode(
    str(data),
    width: final-width,
    quiet-zone: quiet-zone,
    fill: fill,
    background-fill: background,
  )
}
