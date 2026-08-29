// =============================================================================
// A4 INVOICE TEMPLATE (TYPST)
// =============================================================================
// Renders the same document jana2u-pos's frontend used to build client-side
// as `A4Invoice.tsx`, fed by `buildPrintPayload.ts`. The backend's
// `billing::service::print_payload` builds a JSON payload matching the
// contract below and POSTs it to `/api/render/{templateKey}`.
//
// Data contract (see this file's sibling .json for a worked example):
//   logoUrl         string (optional) — an http(s) URL or a data:image/… URI.
//                   The server downloads/decodes it and sets `logo` to a local
//                   filename before compiling; leave `logo` unset in the payload.
//   logo            string (optional, server-injected) — local image filename.
//   logoWidth       number (optional) — logo width in pt (default 46).
//   invoiceNumber, formattedDate, formattedTime   string
//   dueDate         string (optional, credit sales only)
//   cashierName     string
//   status          "paid" | "pending" | "cancelled"
//   isCredit        bool
//   copyDesignation string, e.g. "ORIGINAL — CUSTOMER COPY"
//   isDuplicate     bool
//   customerName, customerPhone, customerAddress   string (optional)
//   paymentMethod   "cash" | "card" | "online" | "split" | "credit"
//   cardLast4, cardRef   string (optional)
//   tenderedAmountCents   integer
//   items           array of { name, sku?, sourceTicketNumber?,
//                     assignedEmployeeName?, quantity, unitPriceCents,
//                     discountCents, totalCents }
//   subtotalCents, discountCents, taxCents, totalCents   integers
//   vatRatePercent  number (optional, e.g. 15 for 15%)
//   amountInWords   string
//   notes           string (optional)
//   warrantyText    string (optional)
//   showBankDetails bool
//   bankName, bankBranch, accountName, accountNumber   string (optional)
//   shopTradingName, shopLegalName, shopEmail, shopWebsite,
//   shopBusinessRegNo, shopVatNo   string (optional)
//   shopIsVatRegistered   bool
//   shopAddressLines   array of string
//   shopPrimaryPhone, shopSecondaryPhone   string (optional)

#import "../lib/money.typ": format-money

#let data = sys.inputs

#set document(title: "Invoice " + data.at("invoiceNumber", default: ""))

// Steel-blue accent (the "modern invoice" identity), semantic status colours
// left as they were.
#let primary = rgb("#2c5f8a")
#let ink = rgb("#1f2024")
#let muted = rgb("#5B6270")
#let muted-2 = rgb("#6C737F")
#let faint = rgb("#9AA1AC")
#let border = rgb("#E2E3E8")
#let panel-bg = rgb("#FAFAFB")
#let panel-bg-2 = rgb("#F4F5F7")
#let ok-color = rgb("#0E9F6E")
#let credit-color = rgb("#C77700")
#let due-color = rgb("#C2334D")

#set page(
  paper: "a4",
  margin: (x: 15mm, top: 15mm, bottom: 15mm),
  fill: white,
)
#set text(font: "Noto Sans", size: 9.5pt, fill: ink, lang: "en")
#set par(leading: 0.55em, justify: false)

// ---------------------------------------------------------------------------
// Logo — the payload image (server-injected from `logoUrl`), or the built-in
// swirl monogram when the shop has none.
// ---------------------------------------------------------------------------
#let logo = data.at("logo", default: none)
#let logo-width = data.at("logoWidth", default: 46) * 1pt
#let default-logo = image(
  bytes(
    "<svg viewBox=\"0 0 100 100\" width=\"48\" height=\"48\" xmlns=\"http://www.w3.org/2000/svg\">"
      + "<path d=\"M 50,16 C 28,16 16,34 16,54 C 16,74 30,84 50,84 C 68,84 82,72 82,52 "
      + "C 82,34 68,28 56,28 C 42,28 34,38 34,49 C 34,60 42,66 50,66 C 58,66 64,59 64,51\" "
      + "fill=\"none\" stroke=\"#2c5f8a\" stroke-width=\"8.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>"
      + "</svg>",
  ),
  format: "svg",
  width: 46pt,
)
#let active-logo = if logo != none { image(logo, width: logo-width) } else { default-logo }

// ---------------------------------------------------------------------------
// Header band: shop branding (left) vs INVOICE meta + status stamp (right)
// ---------------------------------------------------------------------------
#let status = data.at("status", default: "pending")
#let is-credit = data.at("isCredit", default: false) or status == "pending"
#let is-paid = status == "paid" and not is-credit
#let status-label = if is-paid { "PAID" } else if is-credit { "CREDIT" } else { "PENDING" }
#let status-color = if is-paid { ok-color } else if is-credit { credit-color } else { muted-2 }

#grid(
  columns: (1fr, auto),
  column-gutter: 20pt,
  align: (left, right),
  [
    #active-logo
    #v(6pt)
    #let trading-name = data.at("shopTradingName", default: data.at("shopLegalName", default: ""))
    #text(size: 15pt, weight: "bold", fill: primary)[#trading-name]
    #v(2pt)
    #text(size: 9.5pt, weight: "bold")[#data.at("shopLegalName", default: "")]
    #for line in data.at("shopAddressLines", default: ()) [
      #v(1pt)
      #text(size: 8pt, fill: muted)[#line]
    ]
    #v(1pt)
    #let phone = data.at("shopPrimaryPhone", default: "")
    #let phone2 = data.at("shopSecondaryPhone", default: "")
    #text(size: 8pt, fill: muted)[Phone: #phone #if phone2 != "" [| #phone2]]
    #let email = data.at("shopEmail", default: "")
    #if email != "" [
      #v(1pt)
      #text(size: 8pt, fill: muted)[Email: #email #let web = data.at("shopWebsite", default: ""); #if web != "" [| Web: #web]]
    ]
    #let reg-no = data.at("shopBusinessRegNo", default: "")
    #let vat-no = data.at("shopVatNo", default: "")
    #let vat-registered = data.at("shopIsVatRegistered", default: false)
    #if reg-no != "" or (vat-registered and vat-no != "") [
      #v(3pt)
      #text(size: 7.5pt, fill: muted-2)[
        #if reg-no != "" [Reg No: #reg-no #h(4pt)]
        #if vat-registered and vat-no != "" [| VAT No: #vat-no]
      ]
    ]
  ],
  [
    #text(size: 20pt, weight: 800, fill: primary, tracking: 0.06em)[INVOICE]
    #v(4pt)
    #table(
      columns: (auto, auto),
      align: (right, right),
      stroke: none,
      inset: (x: 4pt, y: 1.5pt),
      text(size: 8.5pt, weight: "bold", fill: muted)[Invoice No:], text(size: 8.5pt, weight: "bold")[#data.at("invoiceNumber", default: "")],
      text(size: 8.5pt, weight: "bold", fill: muted)[Date:], text(size: 8.5pt, weight: 600)[#data.at("formattedDate", default: "") #data.at("formattedTime", default: "")],
      ..{
        let due = data.at("dueDate", default: "")
        if due != "" {
          (
            text(size: 8.5pt, weight: "bold", fill: due-color)[Due Date:],
            text(size: 8.5pt, weight: "bold", fill: due-color)[#due],
          )
        } else { () }
      },
      text(size: 8.5pt, weight: "bold", fill: muted)[Cashier:], text(size: 8.5pt)[#data.at("cashierName", default: "Cashier")],
    )
    #v(4pt)
    #box(stroke: 1.5pt + status-color, radius: 3pt, inset: (x: 8pt, y: 3pt))[
      #align(center)[
        #text(weight: 800, size: 10pt, fill: status-color, tracking: 0.08em)[#status-label]
        #if data.at("isDuplicate", default: false) [
          \ #text(size: 6.5pt, fill: status-color, tracking: 0.04em)[DUPLICATE]
        ]
      ]
    ]
  ],
)

#v(4pt)
#line(length: 100%, stroke: 1.5pt + primary)
#v(14pt)

// ---------------------------------------------------------------------------
// Bill To / Payment Summary
// ---------------------------------------------------------------------------
#let info-panel(title, body) = box(
  width: 100%,
  stroke: 0.6pt + border,
  radius: 4pt,
  inset: 9pt,
  fill: panel-bg,
)[
  #text(size: 7.5pt, weight: "bold", fill: muted, tracking: 0.04em)[#upper(title)]
  #v(4pt)
  #body
]

#let customer-name = data.at("customerName", default: "")
#let customer-phone = data.at("customerPhone", default: "")
#let customer-address = data.at("customerAddress", default: "")
#let has-bill-to = customer-name != "" or customer-phone != "" or customer-address != ""

#let bill-to-panel = info-panel("Bill To")[
  #text(size: 10.5pt, weight: "bold")[#customer-name]
  #if customer-phone != "" [
    \ #text(size: 8.5pt, fill: primary, weight: "bold")[Phone: #customer-phone]
  ]
  #if customer-address != "" [
    \ #text(size: 8pt, fill: muted)[Address: #customer-address]
  ]
]

#let payment-summary-panel = info-panel("Payment Summary")[
  #let method = data.at("paymentMethod", default: "cash")
  #let last4 = data.at("cardLast4", default: data.at("cardRef", default: ""))
  #grid(
    columns: (1fr, auto), align: (left, right), row-gutter: 2pt,
    text(size: 8.5pt, fill: muted)[Payment Method:],
    text(size: 8.5pt, weight: "bold")[#upper(method) #if method == "card" and last4 != "" [(•••• #last4)]],
    text(size: 8.5pt, fill: muted)[Amount Tendered:],
    text(size: 8.5pt, weight: 600)[#format-money(data.at("tenderedAmountCents", default: data.at("totalCents", default: 0)))],
    text(size: 8.5pt, fill: muted)[Balance Due:],
    text(size: 8.5pt, weight: "bold", fill: if is-credit { credit-color } else { ok-color })[#if is-credit { format-money(data.at("totalCents", default: 0)) } else { format-money(0) }],
  )
]

// A walk-in with no customer details gets no empty "Bill To" box — the
// payment summary takes the right half on its own instead.
#if has-bill-to {
  grid(columns: (1fr, 1fr), column-gutter: 12pt, bill-to-panel, payment-summary-panel)
} else {
  grid(columns: (1fr, 1fr), column-gutter: 12pt, [], payment-summary-panel)
}

#v(14pt)

// ---------------------------------------------------------------------------
// Items table
// ---------------------------------------------------------------------------
#let items = data.at("items", default: ())

#table(
  columns: (18pt, 1fr, 34pt, 62pt, 52pt, 68pt),
  align: (center, left, center, right, right, right),
  stroke: (x, y) => if y == 0 { none } else { (bottom: 0.5pt + border) },
  inset: (x: 5pt, y: 6pt),
  fill: (x, y) => if y == 0 { primary } else { white },
  table.header(
    text(size: 7.5pt, weight: "bold", fill: white)[#sym.numero],
    text(size: 7.5pt, weight: "bold", fill: white)[Description],
    text(size: 7.5pt, weight: "bold", fill: white)[Qty],
    text(size: 7.5pt, weight: "bold", fill: white)[Unit Price],
    text(size: 7.5pt, weight: "bold", fill: white)[Discount],
    text(size: 7.5pt, weight: "bold", fill: white)[Amount],
  ),
  ..items.enumerate().map(((idx, item)) => (
    text(size: 8.5pt, fill: faint)[#(idx + 1)],
    [
      #text(size: 9pt, weight: 600)[#item.at("name", default: "")]
      #let sku = item.at("sku", default: "")
      #if sku != "" [
        \ #text(size: 7pt, fill: muted-2)[SKU: #sku]
      ]
      #let ticket = item.at("sourceTicketNumber", default: "")
      #if ticket != "" [
        \ #text(size: 7pt, fill: primary)[
          Service Ticket: #ticket
          #let tech = item.at("assignedEmployeeName", default: "")
          #if tech != "" [· Technician: #tech]
        ]
      ]
    ],
    text(size: 8.5pt, weight: 600)[#item.at("quantity", default: 1)],
    text(size: 8.5pt)[#format-money(item.at("unitPriceCents", default: 0))],
    {
      let d = item.at("discountCents", default: 0)
      if d > 0 { text(size: 8.5pt, fill: due-color)[-#format-money(d)] } else { text(size: 8.5pt, fill: faint)[-] }
    },
    text(size: 8.5pt, weight: "bold")[#format-money(item.at("totalCents", default: 0))],
  )).flatten()
)

#v(10pt)

// ---------------------------------------------------------------------------
// Totals
// ---------------------------------------------------------------------------
#align(right)[
  #box(width: 220pt)[
    #grid(
      columns: (1fr, auto), align: (left, right), row-gutter: 3pt,
      text(size: 9pt, fill: muted)[Subtotal:], text(size: 9pt, weight: 600)[#format-money(data.at("subtotalCents", default: 0))],
      ..{
        let d = data.at("discountCents", default: 0)
        if d > 0 {
          (text(size: 9pt, fill: due-color)[Order Discount:], text(size: 9pt, weight: 600, fill: due-color)[-#format-money(d)])
        } else { () }
      },
      ..{
        let t = data.at("taxCents", default: 0)
        if t > 0 {
          let rate = data.at("vatRatePercent", default: none)
          let label = if rate != none { "VAT (" + str(rate) + "%):" } else { "VAT / Tax:" }
          (text(size: 9pt, fill: muted)[#label], text(size: 9pt, weight: 600)[#format-money(t)])
        } else { () }
      },
    )
    #v(5pt)
    #box(width: 100%, fill: primary, radius: 3pt, inset: (x: 10pt, y: 6pt))[
      #grid(
        columns: (1fr, auto), align: (left + horizon, right + horizon),
        text(size: 8pt, weight: "bold", fill: white, tracking: 0.04em)[TOTAL DUE],
        text(size: 13pt, weight: 800, fill: white)[#format-money(data.at("totalCents", default: 0))],
      )
    ]
  ]
]

#v(10pt)

// --- Amount in words ---
#let words = data.at("amountInWords", default: "")
#if words != "" [
  #box(width: 100%, stroke: 0.6pt + border, radius: 3pt, inset: (x: 9pt, y: 6pt), fill: panel-bg)[
    #text(size: 8pt, fill: muted)[Amount in words: ] #text(size: 8pt, weight: "bold", fill: primary)[#words]
  ]
  #v(10pt)
]

// --- Notes ---
#let notes = data.at("notes", default: "")
#if notes != "" [
  #box(width: 100%, stroke: 0.6pt + border, radius: 3pt, inset: (x: 9pt, y: 6pt))[
    #text(size: 8pt, weight: "bold", fill: muted)[Notes: ] #text(size: 8pt)[#notes]
  ]
  #v(10pt)
]

// --- Terms & warranty ---
#let warranty = data.at("warrantyText", default: "")
#if warranty != "" [
  #line(length: 100%, stroke: 0.6pt + border)
  #v(6pt)
  #text(size: 7.5pt, weight: "bold", fill: muted, tracking: 0.04em)[TERMS #sym.amp WARRANTY POLICY]
  #v(3pt)
  #text(size: 7.5pt, fill: muted)[#warranty]
  #v(10pt)
]

// --- Bank details ---
#let show-bank = data.at("showBankDetails", default: false)
#let bank-name = data.at("bankName", default: "")
#if show-bank and bank-name != "" [
  #box(width: 100%, fill: panel-bg-2, radius: 3pt, inset: (x: 9pt, y: 6pt))[
    #text(size: 8pt, weight: "bold")[BANK TRANSFER DETAILS]
    #v(2pt)
    #text(size: 7.5pt)[Bank: #bank-name | Branch: #data.at("bankBranch", default: "")]
    \ #text(size: 7.5pt)[Account Name: #data.at("accountName", default: "") | Account No: #text(weight: "bold")[#data.at("accountNumber", default: "")]]
  ]
  #v(14pt)
]

// --- Signature row ---
#grid(
  columns: (1fr, 1fr),
  align: center,
  [
    #line(length: 70%, stroke: 0.6pt + faint)
    #v(3pt)
    #text(size: 8.5pt, weight: 600)[Authorised Signature]
    \ #text(size: 7.5pt, fill: muted-2)[for #data.at("shopTradingName", default: "")]
  ],
  [
    #line(length: 70%, stroke: 0.6pt + faint)
    #v(3pt)
    #text(size: 8.5pt, weight: 600)[Received By]
    \ #text(size: 7.5pt, fill: muted-2)[Customer Name / Date]
  ],
)

#v(14pt)

// --- Footer ---
#line(length: 100%, stroke: 0.8pt + primary)
#v(5pt)
#align(center)[
  #text(size: 9pt, weight: "bold", fill: primary)[Thank you for your business]
  #v(2pt)
  #text(size: 7.5pt, fill: muted-2)[
    #data.at("shopTradingName", default: "") · Tel: #data.at("shopPrimaryPhone", default: "") · Email: #data.at("shopEmail", default: "")
  ]
  \ #text(size: 7.5pt, weight: 600, fill: muted-2)[
    #data.at("copyDesignation", default: "ORIGINAL — CUSTOMER COPY") · This is a computer-generated invoice.
  ]
]
