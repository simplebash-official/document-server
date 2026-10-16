// Product Sticker Label — shop-designed layouts. Input: the layout (`widthMm`, `heightMm`,
// `elements`) plus the stickers to print (`labels`, each repeated `copies` times); see the
// sibling .schema.json. One page per sticker, the page is exactly the label.
// Every element is absolutely placed in millimetres, so the same template serves any label
// size a shop designs. Barcodes are always Code128, QR codes encode the same `code`.
// Text shrinks to fit its box (never below 4pt), so long names print whole.
#import "../lib/tiaoma/lib.typ" as tiaoma
#import "../lib/qrcode.typ": qrcode
#import "../lib/money.typ": format-money

#let data = sys.inputs

// A runaway payload must not be able to make the engine render thousands of pages.
#let max-pages = 500

#set page(width: data.widthMm * 1mm, height: data.heightMm * 1mm, margin: 0mm)
#set text(font: "Noto Sans")
#set par(leading: 0.35em, spacing: 0pt)

#let text-value(el, label) = {
  let source = el.source
  if source == "productName" {
    label.name
  } else if source == "price" {
    assert(label.priceCents != none, message: "a price element needs a price on every label")
    format-money(label.priceCents)
  } else if source == "sku" {
    label.sku
  } else if source == "code" {
    label.code
  } else if source == "category" {
    label.category
  } else if source == "shopName" {
    data.shopName
  } else if source == "customText" {
    assert(el.customText != none, message: "a custom text element needs its text")
    el.customText
  } else {
    panic("unknown text source: " + source)
  }
}

#let alignment-of(name) = if name == "left" { left } else if name == "center" { center } else { right }

// The smallest text the label printer still prints legibly (matches the designer's minimum).
#let min-font = 4pt

// Shrinks the text in 0.5pt steps until it fits its box, so a long item name is printed
// whole instead of cut off. Only text that would not fit even at `min-font` is clipped.
#let fitted-text(body, width, height, size, weight, alignment) = context {
  let laid(s) = block(width: width, {
    set text(size: s, weight: weight)
    align(alignment, body)
  })
  let s = size
  while s > min-font and measure(laid(s)).height > height { s -= 0.5pt }
  laid(calc.max(s, min-font))
}

#let draw(el, label) = {
  let width = el.widthMm * 1mm
  let height = el.heightMm * 1mm
  let place-at(body) = place(top + left, dx: el.xMm * 1mm, dy: el.yMm * 1mm, body)

  if el.type == "text" {
    place-at(box(width: width, height: height, clip: true, {
      set par(justify: false)
      fitted-text(
        text-value(el, label),
        width,
        height,
        el.fontSizePt * 1pt,
        if el.bold { "bold" } else { "regular" },
        alignment-of(el.align),
      )
    }))
  } else if el.type == "barcode" {
    // The caption is ours (not Zint's) so its size follows the element's font size.
    let caption-height = if el.showValue { el.fontSizePt * 1.25 * 1pt + 2pt } else { 0pt }
    place-at(box(width: width, height: height, clip: true, {
      tiaoma.barcode(
        label.code,
        "Code128",
        width: width,
        height: height - caption-height,
        fit: "stretch",
        options: (show-hrt: false, output-options: (barcode-quiet-zones: true)),
      )
      if el.showValue {
        v(2pt)
        align(center, text(size: el.fontSizePt * 1pt, label.code))
      }
    }))
  } else if el.type == "qr" {
    place-at(box(width: width, height: height, align(center + horizon, qrcode(
      label.code,
      size: calc.min(width, height),
      quiet-zone: 2,
    ))))
  } else if el.type == "line" {
    place-at(rect(width: width, height: height, fill: black, stroke: none))
  } else {
    panic("unknown element type: " + el.type)
  }
}

#let stickers = data.labels.map(label => range(label.copies).map(_ => label)).flatten()
#assert(stickers.len() <= max-pages, message: "too many labels in one job")

#for (index, label) in stickers.enumerate() {
  if index > 0 { pagebreak() }
  for el in data.elements { draw(el, label) }
}
