// =============================================================================
// PROFESSIONAL MODERN INVOICE TEMPLATE (TYPST)
// =============================================================================
// A standalone, brand-forward A4 invoice — not tied to jana2u-pos's billing
// payload. Any caller builds a JSON payload per the sibling .schema.json and
// POSTs it to `/api/render/{templateKey}`.
//
// Money: every amount is an integer number of cents (see `../lib/money.typ`),
// formatted through `format-money` with the `currencySymbol` of the payload.
//
// Logo: `logoUrl` in the payload is a remote image URL. Typst cannot fetch a
// URL at compile time, so document-server downloads it, hands the bytes to
// this one render as a local file, and sets `logo` to that filename before
// compiling — then discards it. Nothing about the image is persisted. When no
// `logoUrl` is given the built-in vector mark below is used.
//
// Data contract (see this file's sibling .json for a worked example):
//   businessName        string (required)
//   businessAddress     array of string
//   logo                string  — local filename, injected by the server from logoUrl; leave unset
//   logoUrl             string  — remote image URL (server-consumed, not read here)
//   logoWidth           number  — logo width in pt (default 48)
//   invoiceTitle        string (default "INVOICE")
//   invoiceDate         string (required)
//   billToTitle         string (default "To :")
//   clientName          string (required)
//   clientAddress       array of string
//   currencySymbol      string (default "Rs.")
//   items               array of { name, quantity (int), unitPriceCents (int) }   (required)
//   note                string
//   subtotalCents       integer (required)
//   discountRatePercent number  — shown in the "DISCOUNT n% :" label only
//   discountCents       integer (default 0)
//   totalCents          integer (required) — the amount due
//   thankYouMessage     string (default "Thank you for your Business")
//   questionsEmail, questionsPhone           string
//   paymentAccount, paymentName, paymentBank string
//   terms               string
// =============================================================================

#import "../lib/money.typ": format-money

#let data = sys.inputs

// ---------------------------------------------------------------------------
// Payload
// ---------------------------------------------------------------------------
#let business-name = data.at("businessName", default: "")
#let business-address = data.at("businessAddress", default: ())
#let logo = data.at("logo", default: none)
#let logo-width = data.at("logoWidth", default: 48) * 1pt
#let invoice-title = data.at("invoiceTitle", default: "INVOICE")
#let invoice-date = data.at("invoiceDate", default: "")
#let bill-to-title = data.at("billToTitle", default: "To :")
#let client-name = data.at("clientName", default: "")
#let client-address = data.at("clientAddress", default: ())
#let currency-symbol = data.at("currencySymbol", default: "Rs.")
#let items = data.at("items", default: ())
#let note = data.at("note", default: "")
#let subtotal-cents = data.at("subtotalCents", default: 0)
#let discount-rate = data.at("discountRatePercent", default: none)
#let discount-cents = data.at("discountCents", default: 0)
#let total-cents = data.at("totalCents", default: 0)
#let thank-you-message = data.at("thankYouMessage", default: "Thank you for your Business")
#let questions-email = data.at("questionsEmail", default: "")
#let questions-phone = data.at("questionsPhone", default: "")
#let payment-account = data.at("paymentAccount", default: "")
#let payment-name = data.at("paymentName", default: "")
#let payment-bank = data.at("paymentBank", default: "")
#let terms = data.at("terms", default: "")

#let money(cents) = format-money(cents, symbol: currency-symbol)
#let discount-label = if discount-rate != none {
  "DISCOUNT " + str(discount-rate) + "% :"
} else {
  "DISCOUNT :"
}

// ---------------------------------------------------------------------------
// Styling
// ---------------------------------------------------------------------------
#let accent-color = rgb("#2c5f8a")
#let text-primary = rgb("#1f2024")
#let text-secondary = rgb("#6c6f7d")
#let line-color = rgb("#e2e3e8")

#set document(title: "Invoice - " + client-name, author: business-name)
#set page(paper: "a4", margin: (x: 2.2cm, top: 1.8cm, bottom: 1.6cm), fill: white)
#set text(font: "Noto Sans", size: 9.5pt, fill: text-primary, lang: "en")
#set par(leading: 0.55em, justify: false)

// ---------------------------------------------------------------------------
// 1. Logo — payload image, or the built-in swirl monogram
// ---------------------------------------------------------------------------
#let default-logo = image(
  bytes(
    "<svg viewBox=\"0 0 100 100\" width=\"48\" height=\"48\" xmlns=\"http://www.w3.org/2000/svg\">"
      + "<path d=\"M 50,16 C 28,16 16,34 16,54 C 16,74 30,84 50,84 C 68,84 82,72 82,52 "
      + "C 82,34 68,28 56,28 C 42,28 34,38 34,49 C 34,60 42,66 50,66 C 58,66 64,59 64,51\" "
      + "fill=\"none\" stroke=\"#2c5f8a\" stroke-width=\"8.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>"
      + "</svg>",
  ),
  format: "svg",
  width: 48pt,
)

#let active-logo = if logo != none {
  image(logo, width: logo-width)
} else {
  default-logo
}

// ---------------------------------------------------------------------------
// 2. Header — business (left) vs invoice / client meta (right)
// ---------------------------------------------------------------------------
#grid(
  columns: (1fr, 1fr),
  align: (left, right),
  [
    #active-logo
    #v(6pt)
    #text(fill: accent-color, size: 16pt, weight: "bold")[#business-name]
    #if business-address.len() > 0 [
      #v(14pt)
      #text(weight: "bold", size: 9.5pt)[#business-address.at(0)]
      #for line in business-address.slice(1) [
        #linebreak()
        #text(fill: text-secondary, size: 8.5pt)[#line]
      ]
    ]
  ],
  [
    #text(fill: accent-color, size: 28pt, weight: "bold")[#invoice-title]
    #v(2pt)
    #text(weight: "bold", size: 10pt)[#invoice-date]
    #v(22pt)
    #text(weight: "bold", size: 9.5pt)[#bill-to-title]
    #linebreak()
    #text(weight: "bold", size: 9.5pt)[#client-name]
    #for line in client-address [
      #linebreak()
      #text(fill: text-secondary, size: 8.5pt)[#line]
    ]
  ],
)

#v(12pt)

// ---------------------------------------------------------------------------
// 3. Items table
// ---------------------------------------------------------------------------
#block(
  fill: accent-color,
  inset: (x: 12pt, y: 9pt),
  width: 100%,
  grid(
    columns: (1fr, 80pt, 50pt, 80pt),
    align: (left + horizon, right + horizon, center + horizon, right + horizon),
    text(fill: white, weight: "bold", size: 9pt)[Items Description],
    text(fill: white, weight: "bold", size: 9pt)[Unit Price],
    text(fill: white, weight: "bold", size: 9pt)[Qnt],
    text(fill: white, weight: "bold", size: 9pt)[Total],
  ),
)

#for item in items {
  let qty = item.at("quantity", default: 1)
  let unit-cents = item.at("unitPriceCents", default: 0)
  let line-cents = unit-cents * qty
  block(
    inset: (x: 12pt, y: 6pt),
    width: 100%,
    grid(
      columns: (1fr, 80pt, 50pt, 80pt),
      align: (left + horizon, right + horizon, center + horizon, right + horizon),
      text(weight: "bold", size: 9.5pt)[#item.at("name", default: "")],
      text(weight: "bold", size: 9.5pt)[#money(unit-cents)],
      text(size: 9.5pt)[#qty],
      text(weight: "bold", size: 9.5pt)[#money(line-cents)],
    ),
  )
  line(length: 100%, stroke: 0.5pt + line-color)
}

#v(10pt)

// ---------------------------------------------------------------------------
// 4. Note + financial summary
// ---------------------------------------------------------------------------
#grid(
  columns: (1.1fr, 1fr),
  column-gutter: 20pt,
  [
    #if note != "" [
      #text(weight: "bold", size: 9pt)[Note:]
      #v(3pt)
      #text(fill: text-secondary, size: 7.8pt)[#note]
    ]
  ],
  [
    #align(right)[
      #block(width: 100%)[
        #grid(
          columns: (1fr, auto),
          row-gutter: 7pt,
          align: (left, right),
          text(weight: "bold", size: 8.5pt)[SUBTOTAL :],
          text(weight: "bold", size: 8.5pt)[#money(subtotal-cents)],
          text(weight: "bold", size: 8.5pt)[#discount-label],
          text(weight: "bold", size: 8.5pt)[#money(discount-cents)],
        )
        #v(8pt)
        #block(
          fill: accent-color,
          inset: (x: 12pt, y: 10pt),
          width: 100%,
          grid(
            columns: (1fr, auto),
            align: (left + horizon, right + horizon),
            text(fill: white, weight: "bold", size: 10pt)[TOTAL DUE :],
            text(fill: white, weight: "bold", size: 10pt)[#money(total-cents)],
          ),
        )
      ]
    ]
  ],
)

#v(14pt)

// ---------------------------------------------------------------------------
// 5. Closing message + footer (kept together on one page)
// ---------------------------------------------------------------------------
#block(breakable: false)[
  #text(fill: accent-color, size: 12pt, weight: "bold")[#thank-you-message]

  #v(8pt)
  #line(length: 100%, stroke: 0.6pt + text-primary)
  #v(8pt)

  #grid(
    columns: (1fr, 1.1fr, 1.25fr),
    column-gutter: 15pt,
    [
      #text(weight: "bold", size: 9pt)[Questions?]
      #v(5pt)
      #grid(
        columns: (auto, auto),
        column-gutter: 6pt,
        row-gutter: 3pt,
        text(fill: text-secondary, size: 7.8pt)[Email us],
        text(size: 7.8pt)[: #questions-email],
        text(fill: text-secondary, size: 7.8pt)[Call us],
        text(size: 7.8pt)[: #questions-phone],
      )
    ],
    [
      #text(weight: "bold", size: 9pt)[Payment Info :]
      #v(5pt)
      #grid(
        columns: (auto, auto),
        column-gutter: 6pt,
        row-gutter: 3pt,
        text(fill: text-secondary, size: 7.8pt)[Account],
        text(size: 7.8pt)[: #payment-account],
        text(fill: text-secondary, size: 7.8pt)[A/C Name],
        text(size: 7.8pt)[: #payment-name],
        text(fill: text-secondary, size: 7.8pt)[Bank Detail],
        text(size: 7.8pt)[: #payment-bank],
      )
    ],
    [
      #text(weight: "bold", size: 9pt)[Terms & Conditions/Note:]
      #v(5pt)
      #text(fill: text-secondary, size: 7.8pt)[#terms]
    ],
  )
]
