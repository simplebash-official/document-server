// =============================================================================
// CREDIT NOTE TEMPLATE (TYPST)
// =============================================================================
// Renders a customer-facing Credit Note for a return/refund/exchange, backed
// by jana2u-pos's `CreditNote` domain (a linked document against an invoice,
// never a mutation of the invoice itself — see backend CLAUDE.md's Credit
// Note section). The backend's `billing::service::print_payload` builds a
// JSON payload matching the contract below and POSTs it to
// `/api/render/{templateKey}`.
//
// Visual conventions (palette, fonts, money formatting, panel/table style)
// deliberately mirror the A4 Invoice template rather than inventing a new look.
//
// Data contract (see this file's sibling .json for a worked example):
//   creditNoteNumber   string
//   formattedDate      string
//   cashierName        string
//   originalInvoiceNumber   string (optional — omitted/empty when noReceipt)
//   noReceipt          bool
//   isManagerOverride  bool
//   exchangeReference  string (optional — presence marks this as an exchange)
//   customerName, customerPhone   string (optional)
//   items              array of { name, sku?, serialNumber?, quantity,
//                         condition, disposition?, unitPriceCents,
//                         totalCents }
//                       — `condition`/`disposition` arrive as already
//                       human-readable labels (e.g. "Good — Resalable",
//                       "Return to Supplier"), not raw enum values; the
//                       template only styles them, it doesn't translate them.
//   refundCashCents        integer
//   balanceReductionCents  integer
//   refundBreakdown    array of { method, amountCents } (only rendered as
//                       its own itemized block when it has more than one leg
//                       — a single-method refund is already summarized by
//                       refundCashCents above)
//   notes              string (optional)
//   shopTradingName, shopLegalName, shopEmail, shopWebsite,
//   shopBusinessRegNo, shopVatNo   string (optional)
//   shopIsVatRegistered   bool
//   shopAddressLines   array of string
//   shopPrimaryPhone, shopSecondaryPhone   string (optional)

#import "../lib/money.typ": format-money

#let data = sys.inputs

#set document(title: "Credit Note " + data.at("creditNoteNumber", default: ""))

#let primary = rgb("#3D4EAC")
#let ink = rgb("#0F1115")
#let muted = rgb("#5B6270")
#let muted-2 = rgb("#6C737F")
#let faint = rgb("#9AA1AC")
#let border = rgb("#E4E6EB")
#let panel-bg = rgb("#FAFAFB")
#let panel-bg-2 = rgb("#F4F5F7")
#let ok-color = rgb("#0E9F6E")
#let warn-color = rgb("#C77700")
#let due-color = rgb("#C2334D")
#let exchange-color = rgb("#4C6EF5")
#let override-color = rgb("#AE3EC9")

#set page(
  paper: "a4",
  margin: (x: 15mm, top: 15mm, bottom: 15mm),
  fill: white,
)
#set text(font: "Noto Sans", size: 9.5pt, fill: ink, lang: "en")
#set par(leading: 0.55em, justify: false)

// ---------------------------------------------------------------------------
// Header band: shop branding (left) vs CREDIT NOTE meta + flag badges (right)
// ---------------------------------------------------------------------------
#let no-receipt = data.at("noReceipt", default: false)
#let is-override = data.at("isManagerOverride", default: false)
#let exchange-ref = data.at("exchangeReference", default: "")
#let is-exchange = exchange-ref != ""

#grid(
  columns: (1fr, auto),
  column-gutter: 20pt,
  align: (left, right),
  [
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
    #text(size: 20pt, weight: 800, fill: due-color, tracking: 0.06em)[CREDIT NOTE]
    #v(4pt)
    #table(
      columns: (auto, auto),
      align: (right, right),
      stroke: none,
      inset: (x: 4pt, y: 1.5pt),
      text(size: 8.5pt, weight: "bold", fill: muted)[Credit Note No:], text(size: 8.5pt, weight: "bold")[#data.at("creditNoteNumber", default: "")],
      text(size: 8.5pt, weight: "bold", fill: muted)[Date:], text(size: 8.5pt, weight: 600)[#data.at("formattedDate", default: "")],
      text(size: 8.5pt, weight: "bold", fill: muted)[Issued By:], text(size: 8.5pt)[#data.at("cashierName", default: "Cashier")],
    )
    #v(6pt)
    #stack(
      dir: ltr,
      spacing: 4pt,
      ..{
        let badges = ()
        if is-exchange {
          badges.push(box(stroke: 1.2pt + exchange-color, radius: 3pt, inset: (x: 6pt, y: 2pt))[
            #text(weight: 800, size: 7.5pt, fill: exchange-color, tracking: 0.05em)[EXCHANGE]
          ])
        }
        if no-receipt {
          badges.push(box(stroke: 1.2pt + due-color, radius: 3pt, inset: (x: 6pt, y: 2pt))[
            #text(weight: 800, size: 7.5pt, fill: due-color, tracking: 0.05em)[NO RECEIPT]
          ])
        }
        if is-override {
          badges.push(box(stroke: 1.2pt + override-color, radius: 3pt, inset: (x: 6pt, y: 2pt))[
            #text(weight: 800, size: 7.5pt, fill: override-color, tracking: 0.05em)[MANAGER APPROVED]
          ])
        }
        badges
      }
    )
  ],
)

#v(4pt)
#line(length: 100%, stroke: 1.5pt + due-color)
#v(14pt)

// ---------------------------------------------------------------------------
// Original Sale / Customer
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

#grid(
  columns: (1fr, 1fr),
  column-gutter: 12pt,
  info-panel("Original Sale")[
    #let orig = data.at("originalInvoiceNumber", default: "")
    #if no-receipt or orig == "" [
      #text(size: 10.5pt, weight: "bold", fill: due-color)[No Receipt Provided]
      #v(2pt)
      #text(size: 7.5pt, fill: muted)[Valued at current selling price, approved by a manager.]
    ] else [
      #text(size: 10.5pt, weight: "bold")[Invoice #orig]
    ]
  ],
  info-panel("Customer")[
    #text(size: 10.5pt, weight: "bold")[#data.at("customerName", default: "Walk-in Customer")]
    #let phone = data.at("customerPhone", default: "")
    #if phone != "" [
      \ #text(size: 8.5pt, fill: primary, weight: "bold")[Phone: #phone]
    ]
  ],
)

#v(14pt)

// ---------------------------------------------------------------------------
// Items table
// ---------------------------------------------------------------------------
#let items = data.at("items", default: ())

#table(
  columns: (18pt, 1fr, 34pt, 62pt, 68pt),
  align: (center, left, center, right, right),
  stroke: (x, y) => if y == 0 { (top: 0.6pt + border, bottom: 0.6pt + border) } else { (bottom: 0.6pt + border) },
  inset: (x: 5pt, y: 6pt),
  fill: (x, y) => if y == 0 { panel-bg-2 } else { white },
  table.header(
    text(size: 7.5pt, weight: "bold", fill: muted)[#sym.numero],
    text(size: 7.5pt, weight: "bold", fill: muted)[Item Returned],
    text(size: 7.5pt, weight: "bold", fill: muted)[Qty],
    text(size: 7.5pt, weight: "bold", fill: muted)[Unit Price],
    text(size: 7.5pt, weight: "bold", fill: muted)[Amount],
  ),
  ..items.enumerate().map(((idx, item)) => (
    text(size: 8.5pt, fill: faint)[#(idx + 1)],
    [
      #text(size: 9pt, weight: 600)[#item.at("name", default: "")]
      #let sku = item.at("sku", default: "")
      #if sku != "" [
        \ #text(size: 7pt, fill: muted-2)[SKU: #sku]
      ]
      #let serial = item.at("serialNumber", default: "")
      #if serial != "" [
        \ #text(size: 7pt, fill: muted-2)[Serial: #serial]
      ]
      #let condition = item.at("condition", default: "")
      #let disposition = item.at("disposition", default: "")
      #if condition != "" [
        \ #text(size: 7pt, weight: "bold", fill: warn-color)[
          Condition: #condition
          #if disposition != "" [· #disposition]
        ]
      ]
    ],
    text(size: 8.5pt, weight: 600)[#item.at("quantity", default: 1)],
    text(size: 8.5pt)[#format-money(item.at("unitPriceCents", default: 0))],
    text(size: 8.5pt, weight: "bold")[#format-money(item.at("totalCents", default: 0))],
  )).flatten()
)

#v(10pt)

// ---------------------------------------------------------------------------
// Refund summary
// ---------------------------------------------------------------------------
#let refund-cash = data.at("refundCashCents", default: 0)
#let balance-reduction = data.at("balanceReductionCents", default: 0)
#let breakdown = data.at("refundBreakdown", default: ())

#align(right)[
  #box(width: 260pt)[
    #box(width: 100%, fill: if refund-cash > 0 { ok-color } else { panel-bg-2 }, radius: 3pt, inset: (x: 10pt, y: 6pt))[
      #grid(
        columns: (1fr, auto), align: (left + horizon, right + horizon),
        text(size: 8pt, weight: "bold", fill: if refund-cash > 0 { white } else { muted }, tracking: 0.04em)[CASH / CARD REFUNDED],
        text(size: 13pt, weight: 800, fill: if refund-cash > 0 { white } else { muted })[#format-money(refund-cash)],
      )
    ]
    #if breakdown.len() > 1 [
      #v(4pt)
      #grid(
        columns: (1fr, auto), align: (left, right), row-gutter: 2pt,
        ..breakdown.map(leg => (
          text(size: 7.5pt, fill: muted)[via #upper(leg.at("method", default: ""))],
          text(size: 7.5pt, weight: 600)[#format-money(leg.at("amountCents", default: 0))],
        )).flatten()
      )
    ]
    #if balance-reduction > 0 [
      #v(6pt)
      #box(width: 100%, stroke: 0.6pt + warn-color, radius: 3pt, inset: (x: 10pt, y: 6pt))[
        #grid(
          columns: (1fr, auto), align: (left + horizon, right + horizon),
          text(size: 8pt, weight: "bold", fill: warn-color, tracking: 0.04em)[APPLIED TO REDUCE BALANCE OWED],
          text(size: 11pt, weight: 800, fill: warn-color)[#format-money(balance-reduction)],
        )
      ]
      #v(3pt)
      #text(size: 7pt, fill: muted)[No cash changes hands for this portion — it lowers what is still owed on the original sale.]
    ]
  ]
]

#v(10pt)

// --- Notes ---
#let notes = data.at("notes", default: "")
#if notes != "" [
  #box(width: 100%, stroke: 0.6pt + border, radius: 3pt, inset: (x: 9pt, y: 6pt))[
    #text(size: 8pt, weight: "bold", fill: muted)[Notes: ] #text(size: 8pt)[#notes]
  ]
  #v(10pt)
]

// --- Manager override notice ---
#if is-override [
  #box(width: 100%, stroke: 0.6pt + override-color, radius: 3pt, inset: (x: 9pt, y: 6pt), fill: rgb("#FBF3FD"))[
    #text(size: 7.5pt, weight: "bold", fill: override-color)[Manager Approval: ]
    #text(size: 7.5pt, fill: muted)[This credit note required manager approval (outside the normal return window and/or issued without a receipt).]
  ]
  #v(10pt)
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
    #text(size: 8.5pt, weight: 600)[Customer Signature]
    \ #text(size: 7.5pt, fill: muted-2)[Customer Name / Date]
  ],
)

#v(14pt)

// --- Footer ---
#line(length: 100%, stroke: 0.6pt + border)
#v(4pt)
#align(center)[
  #text(size: 7.5pt, fill: muted-2)[
    #data.at("shopTradingName", default: "") · Tel: #data.at("shopPrimaryPhone", default: "") · Email: #data.at("shopEmail", default: "")
  ]
  \ #text(size: 7.5pt, weight: 600, fill: muted-2)[
    This is a computer-generated credit note and confirms the return described above.
  ]
]
