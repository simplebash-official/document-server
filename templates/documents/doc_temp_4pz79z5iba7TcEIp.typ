// =============================================================================
// THERMAL RECEIPT TEMPLATE (TYPST)
// =============================================================================
// Renders the same document jana2u-pos's frontend used to build client-side
// as `ThermalReceipt.tsx`, fed by `buildPrintPayload.ts`. The backend's
// `billing::service::print_payload` builds a JSON payload matching the
// contract below and POSTs it to `/api/render/{templateKey}`.
//
// Data contract (see thermal-receipt.json for a worked example):
//   paperWidthMm    number, 58 or 80 — the physical roll width. Mirrors the
//                   frontend's PAPER_PROFILES.thermal58/thermal80 (D9: the
//                   frontend already lets a cashier pick either width, so
//                   this template must not hard-code one).
//   invoiceNumber   string
//   formattedDate   string, e.g. "17 Aug 2026"
//   formattedTime   string, e.g. "14:32"
//   cashierName     string
//   customerName    string (optional)
//   customerPhone   string (optional)
//   items           array of { name, sku?, quantity, unitPriceCents,
//                     discountCents, totalCents, sourceTicketNumber? }
//   subtotalCents, discountCents, taxCents, totalCents   integers
//   paymentMethod   "cash" | "card" | "online" | "split" | "credit"
//   tenderedAmountCents, changeDueCents   integers (cash only)
//   cardLast4       string (card only)
//   splitPayments   array of { method, amountCents, cardLast4? } (split only)
//   isCredit        bool
//   warrantyText    string (optional)
//   footerText      string (optional) — thank-you line
//   shopTradingName, shopLegalName   string
//   shopAddressLines   array of string
//   shopPrimaryPhone, shopSecondaryPhone   string (optional)

#import "../lib/codes.typ": barcode
#import "../lib/money.typ": format-money

#let data = sys.inputs

#set document(title: "Receipt " + data.at("invoiceNumber", default: ""))

// --- Paper width (D9): the frontend already switches between 58mm and
// 80mm thermal rolls per-terminal — this template must follow, not assume
// one width. `printableWidthMm`/`paddingMm` mirror the frontend's
// PAPER_PROFILES entries for each width.
#let paper-width-mm = data.at("paperWidthMm", default: 80)
#let is-58mm = paper-width-mm <= 58
#let side-margin = if is-58mm { 1mm } else { 2mm }
#let base-size = if is-58mm { 8.5pt } else { 9.5pt }

#set page(
  width: paper-width-mm * 1mm,
  height: auto,
  margin: (x: side-margin, top: 4mm, bottom: 4mm),
  fill: white,
)

#set text(font: "Noto Sans", size: base-size, fill: black, lang: "en")
#set par(leading: 0.5em, justify: false)

#let divider(dashed: true) = {
  v(2.5pt)
  line(
    length: 100%,
    stroke: (
      paint: black,
      thickness: 0.6pt,
      dash: if dashed { (array: (1.5pt, 1.5pt)) } else { "solid" },
    ),
  )
  v(2.5pt)
}

#let kv-row(label, value, bold-value: false, size: base-size) = grid(
  columns: (1fr, auto),
  align: (left, right),
  text(size: size)[#label],
  text(size: size, weight: if bold-value { "bold" } else { "regular" })[#value],
)

// --- Header ---
#align(center)[
  #text(weight: "bold", size: base-size + 3pt)[#data.at("shopTradingName", default: data.at("shopLegalName", default: "Receipt"))]
  #v(1pt)
  #for line in data.at("shopAddressLines", default: ()) [
    #text(size: base-size - 1.5pt)[#line] \
  ]
  #let phone = data.at("shopPrimaryPhone", default: "")
  #let phone2 = data.at("shopSecondaryPhone", default: "")
  #if phone != "" [
    #text(size: base-size - 1.5pt)[Tel: #phone #if phone2 != "" [| #phone2]]
  ]
]

#divider(dashed: true)

// --- Meta ---
#kv-row("Invoice #:", data.at("invoiceNumber", default: ""), bold-value: true)
#kv-row("Date:", data.at("formattedDate", default: "") + " " + data.at("formattedTime", default: ""))
#kv-row("Cashier:", data.at("cashierName", default: "Cashier"))
#let customer-name = data.at("customerName", default: "")
#if customer-name != "" [
  #kv-row("Customer:", customer-name)
]
#let customer-phone = data.at("customerPhone", default: "")
#if customer-phone != "" [
  #kv-row("Phone:", customer-phone)
]

#divider(dashed: true)

// --- Line items ---
#let items = data.at("items", default: ())
#for (idx, item) in items.enumerate() [
  #let name = item.at("name", default: "")
  #let sku = item.at("sku", default: "")
  #let qty = item.at("quantity", default: 1)
  #let unit-price = item.at("unitPriceCents", default: 0)
  #let line-discount = item.at("discountCents", default: 0)
  #let line-total = item.at("totalCents", default: 0)
  #let ticket = item.at("sourceTicketNumber", default: "")

  #text(weight: "bold")[#(idx + 1). #name #if sku != "" [(#sku)]]
  #if ticket != "" [
    \ #text(size: base-size - 1.5pt)[Ticket: #ticket]
  ]
  \ #grid(
    columns: (1fr, auto),
    align: (left, right),
    text(size: base-size - 0.5pt)[#qty × #format-money(unit-price)],
    text(weight: "bold")[#format-money(line-total)],
  )
  #if line-discount > 0 [
    #grid(
      columns: (1fr, auto),
      align: (left, right),
      text(size: base-size - 1.5pt)[Line Discount],
      text(size: base-size - 1.5pt)[-#format-money(line-discount)],
    )
  ]
  #v(2pt)
]

#divider(dashed: true)

// --- Totals ---
#kv-row("Subtotal:", format-money(data.at("subtotalCents", default: 0)))
#let order-discount = data.at("discountCents", default: 0)
#if order-discount > 0 [
  #kv-row("Order Discount:", "-" + format-money(order-discount))
]
#let tax = data.at("taxCents", default: 0)
#if tax > 0 [
  #kv-row("VAT / Tax:", format-money(tax))
]

#divider(dashed: false)

#kv-row("TOTAL:", format-money(data.at("totalCents", default: 0)), bold-value: true, size: base-size + (if is-58mm { 4pt } else { 5pt }))

#divider(dashed: false)

// --- Payment details ---
#let payment-method = data.at("paymentMethod", default: "cash")
#kv-row("Payment Method:", upper(payment-method), bold-value: true)

#if payment-method == "cash" [
  #kv-row("Tendered:", format-money(data.at("tenderedAmountCents", default: 0)))
  #kv-row("CHANGE DUE:", format-money(data.at("changeDueCents", default: 0)), bold-value: true)
]

#let card-last4 = data.at("cardLast4", default: "")
#if payment-method == "card" and card-last4 != "" [
  #kv-row("Card Last 4:", "•••• " + card-last4, bold-value: true)
]

#if payment-method == "split" [
  #for leg in data.at("splitPayments", default: ()) [
    #let method = upper(leg.at("method", default: ""))
    #let last4 = leg.at("cardLast4", default: "")
    #let label = "Split (" + method + (if last4 != "" [ •••• #last4] else []) + "):"
    #kv-row(label, format-money(leg.at("amountCents", default: 0)))
  ]
]

#if data.at("isCredit", default: false) [
  #kv-row("STATUS:", "UNPAID CREDIT", bold-value: true)
]

#divider(dashed: true)

// --- Warranty ---
#let warranty = data.at("warrantyText", default: "")
#if warranty != "" [
  #align(center)[#text(size: base-size - 2pt)[#warranty]]
  #divider(dashed: true)
]

// --- Barcode + thank you ---
#align(center)[
  #barcode(data.at("invoiceNumber", default: ""), height: 8mm, show-text: true)
  #v(3pt)
  #text(weight: "bold")[#data.at("footerText", default: "Thank you for your business!")]
]

#v(12mm)
