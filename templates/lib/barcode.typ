// Dedicated Barcode Component for Typst templates.
// Wraps the vendored `tiaoma` package for 1D linear barcodes (Code128, Code39, EAN13, etc.).

#import "tiaoma/lib.typ" as tiaoma

/// Renders a 1D barcode with optional human-readable text underneath.
/// - `data`: The string or value to encode in the barcode.
/// - `symbology`: Barcode format ("Code128", "Code39", "EAN13", "EAN8"). Defaults to "Code128".
/// - `height`: Barcode height (defaults to 10mm).
/// - `width`: Barcode width (defaults to auto).
/// - `show-text`: Whether to display the encoded text below the barcode (defaults to true).
/// - `caption`: Custom text to display instead of the raw data string (defaults to data).
/// - `text-size`: Font size for the text caption (defaults to 6pt).
/// - `font`: Font family for the caption (defaults to "Noto Sans").
#let barcode(
  data,
  symbology: "Code128",
  height: 10mm,
  width: auto,
  show-text: true,
  caption: none,
  text-size: 6pt,
  font: "Noto Sans",
) = {
  if data == none or str(data).trim() == "" {
    return
  }

  let val = str(data)
  let sym = if symbology == "code128" or symbology == "Code 128" {
    "Code128"
  } else if symbology == "code39" or symbology == "Code 39" {
    "Code39"
  } else if symbology == "ean13" or symbology == "EAN-13" {
    "EAN13"
  } else if symbology == "ean8" or symbology == "EAN-8" {
    "EAN8"
  } else {
    symbology
  }

  let code-content = tiaoma.barcode(val, sym, height: height, width: width)

  if show-text or caption != none {
    let display-text = if caption != none { str(caption) } else { val }
    align(center)[
      #code-content
      #v(1.5pt)
      #text(size: text-size, font: font, weight: "medium")[#display-text]
    ]
  } else {
    code-content
  }
}
