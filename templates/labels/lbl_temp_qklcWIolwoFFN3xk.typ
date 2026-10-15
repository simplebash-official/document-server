// Product Sticker Label — shop-designed layouts. Input: the layout (`widthMm`, `heightMm`,
// `elements`) plus the stickers to print (`labels`, each repeated `copies` times); see the
// sibling .schema.json. One page per sticker, the page is exactly the label.
// Every element is absolutely placed in millimetres, so the same template serves any label
// size a shop designs. Barcodes are always Code128, QR codes encode the same `code`.
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

#let draw(el, label) = {
  let width = el.widthMm * 1mm
  let height = el.heightMm * 1mm
  let place-at(body) = place(top + left, dx: el.xMm * 1mm, dy: el.yMm * 1mm, body)

  if el.type == "text" {
    place-at(box(width: width, height: height, clip: true, {
      set text(size: el.fontSizePt * 1pt, weight: if el.bold { "bold" } else { "regular" })
      set par(justify: false)
      align(alignment-of(el.align), text-value(el, label))
    }))
  } else if el.type == "barcode" {
    // The caption is ours (not Zint's) so its size follows the element's font size.
    let caption-height = if el.showValue { el.fontSizePt * 1.25 * 1pt + 1pt } else { 0pt }
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
        v(1pt)
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
