// =============================================================================
// ANALYTICS REPORT TEMPLATE (TYPST)
// =============================================================================
// The multi-section business analytics PDF for jana2u-pos's "Analytics &
// Reports" screen. The backend's `reports::service::report_payload::
// build_analytics_report_data` aggregates everything and POSTs a payload
// matching the contract below to `/api/render/{templateKey}`; this template
// does no arithmetic beyond formatting and bar scaling.
//
// Charts are hand-drawn via `../lib/charts.typ` (no cetz — see that file's
// header). Money always arrives as integer cents.
//
// Data contract (see the sibling .schema.json for the enforced version and
// .json for a worked example):
//   logoUrl            string (optional) — server downloads/decodes, sets `logo`
//   logo               string (optional, server-injected) — local filename
//   shopName           string          — from stored template-data
//   shopAddressLines   array of string  — from stored template-data
//   generatedAt        string           — "30 Aug 2026, 14:05"
//   periodLabel        string           — "1 Aug 2026 – 30 Aug 2026"
//   granularityLabel   string           — "Weekly"
//   comparisonLabel    string (optional)— "vs previous 30 days"
//   currency           string (optional)— "LKR"
//   kpis               array of { label, value, deltaLabel?, deltaDirection? }
//   timeseries         { labels[], revenueCents[], retailRevenueCents[],
//                        repairRevenueCents[], printRevenueCents[], cogsCents[],
//                        grossProfitCents[], grossMarginBps[] }
//   paymentMethods     array of { label, amountCents }
//   salesByCategory    array of { name, unitsSold, revenueCents, grossProfitCents }
//   topProducts        array of { name, unitsSold, revenueCents }
//   topCustomers       array of { name, invoiceCount, revenueCents,
//                        grossProfitCents, outstandingCents }
//   cashierPerformance array of { name, invoiceCount, revenueCents,
//                        discountGivenCents, grossProfitCents }
//   receivablesAging   array of { label, amountCents, invoiceCount }
//   weekdayPattern     array of 7 numbers (revenue per weekday, Mon..Sun)
//   refunds            { creditNoteCount, netRefundCents, refundCashCents,
//                        byReason: array of { reason, amountCents, count } }
//   inventoryValuation { totalCostValuationCents, totalRetailValuationCents,
//                        potentialGrossProfitCents, lowStockProductsCount,
//                        outOfStockProductsCount,
//                        categories: array of { name, costValuationCents,
//                        retailValuationCents } }

#import "../lib/money.typ": format-money
#import "../lib/charts.typ": column-chart, stacked-column-chart, hbar-chart, line-chart, share-bar

#let data = sys.inputs

// --- palette ---------------------------------------------------------------
#let primary = rgb("#3D4EAC")
#let ink = rgb("#1f2024")
#let muted = rgb("#5B6270")
#let faint = rgb("#9AA1AC")
#let border = rgb("#E2E3E8")
#let panel = rgb("#F4F5F7")
#let ok = rgb("#0E9F6E")
#let bad = rgb("#C2334D")
#let c-retail = rgb("#3D4EAC")
#let c-repair = rgb("#E8833A")
#let c-print = rgb("#1FA2A2")
#let c-profit = rgb("#0E9F6E")
#let c-cogs = rgb("#C2334D")

#let currency = data.at("currency", default: "LKR")
#let shop-name = data.at("shopName", default: "")

// cents -> rupees number, for the chart primitives
#let rupees(cents) = cents / 100

#set document(title: "Analytics Report — " + data.at("periodLabel", default: ""))
#set page(
  paper: "a4",
  margin: (x: 15mm, top: 16mm, bottom: 15mm),
  fill: white,
  header: context {
    if counter(page).get().first() > 1 {
      set text(size: 7.5pt, fill: faint)
      grid(
        columns: (1fr, auto),
        align: (left, right),
        [#shop-name],
        [Analytics Report · #data.at("periodLabel", default: "")],
      )
      line(start: (0pt, 4pt), end: (100%, 4pt), stroke: 0.5pt + border)
    }
  },
  footer: context {
    set text(size: 7.5pt, fill: faint)
    grid(
      columns: (1fr, auto),
      align: (left, right),
      [Generated #data.at("generatedAt", default: "")],
      [Page #counter(page).display() of #counter(page).final().first()],
    )
  },
)
#set text(font: "Noto Sans", size: 9pt, fill: ink, lang: "en")
#set par(leading: 0.55em)

#let section-title(body) = {
  v(6pt)
  block(text(size: 12pt, weight: 800, fill: ink, body))
  v(3pt)
  line(start: (0pt, 0pt), end: (100%, 0pt), stroke: 0.8pt + primary)
  v(6pt)
}

#let table-header(..cells) = table.header(
  ..cells.pos().map(c => text(size: 7pt, weight: "bold", fill: muted)[#c]),
)

#let reason-labels = (
  "defective": "Defective",
  "wrong_item": "Wrong item",
  "customer_changed_mind": "Changed mind",
  "warranty_claim": "Warranty claim",
  "other": "Other",
)
#let humanize-reason(r) = reason-labels.at(r, default: r)

// =========================================================================
// COVER
// =========================================================================
#align(center + horizon)[
  #text(size: 28pt, weight: 800, fill: primary)[Analytics & Reports]
  #v(10pt)
  #if shop-name != "" {
    text(size: 14pt, weight: 700)[#shop-name]
    linebreak()
  }
  #for l in data.at("shopAddressLines", default: ()) {
    text(size: 9pt, fill: muted)[#l]
    linebreak()
  }
  #v(10pt)
  #text(size: 12pt, weight: 600)[#data.at("periodLabel", default: "")]
  #linebreak()
  #text(size: 9pt, fill: muted)[
    #data.at("granularityLabel", default: "")
    #let cmp = data.at("comparisonLabel", default: "")
    #if cmp != "" [ · #cmp ]
  ]
]
#pagebreak()

// =========================================================================
// EXECUTIVE SUMMARY — KPI grid
// =========================================================================
#section-title[Executive summary]

#let kpis = data.at("kpis", default: ())
#grid(
  columns: (1fr, 1fr, 1fr),
  gutter: 8pt,
  ..kpis.map(k => block(
    width: 100%,
    inset: 9pt,
    radius: 4pt,
    stroke: 0.6pt + border,
    fill: panel,
    {
      text(size: 7pt, weight: "bold", fill: muted, tracking: 0.03em)[#upper(k.at("label"))]
      v(3pt)
      text(size: 14pt, weight: 800)[#k.at("value")]
      let d = k.at("deltaLabel", default: "")
      if d != "" {
        v(2pt)
        let dir = k.at("deltaDirection", default: "flat")
        text(size: 7pt, fill: if dir == "up" { ok } else if dir == "down" { bad } else { muted })[#d]
      }
    },
  )),
)

// =========================================================================
// REVENUE & PROFIT TREND
// =========================================================================
#section-title[Revenue & profit trend]

#let ts = data.at("timeseries", default: (:))
#let ts-labels = ts.at("labels", default: ())

#if ts-labels.len() > 0 [
  #text(size: 8.5pt, weight: 600, fill: muted)[Revenue by stream]
  #v(3pt)
  #stacked-column-chart(
    ts-labels,
    (
      (name: "Retail", color: c-retail, values: ts.at("retailRevenueCents", default: ()).map(rupees)),
      (name: "Repairs", color: c-repair, values: ts.at("repairRevenueCents", default: ()).map(rupees)),
      (name: "Print", color: c-print, values: ts.at("printRevenueCents", default: ()).map(rupees)),
    ),
    height: 92pt,
  )
  #v(3pt)
  #stack(
    dir: ltr,
    spacing: 12pt,
    ..(("Retail", c-retail), ("Repairs", c-repair), ("Print", c-print)).map(((l, c)) => stack(
      dir: ltr,
      spacing: 4pt,
      box(width: 7pt, height: 7pt, radius: 1pt, fill: c),
      text(size: 6.5pt, fill: muted)[#l],
    )),
  )
  #v(10pt)

  #text(size: 8.5pt, weight: 600, fill: muted)[Revenue vs gross profit]
  #v(3pt)
  #line-chart(
    ts-labels,
    (
      (name: "Revenue", color: primary, values: ts.at("revenueCents", default: ()).map(rupees)),
      (name: "Gross profit", color: c-profit, values: ts.at("grossProfitCents", default: ()).map(rupees)),
    ),
    height: 96pt,
  )
  #v(3pt)
  #stack(
    dir: ltr,
    spacing: 12pt,
    ..(("Revenue", primary), ("Gross profit", c-profit)).map(((l, c)) => stack(
      dir: ltr,
      spacing: 4pt,
      box(width: 7pt, height: 7pt, radius: 1pt, fill: c),
      text(size: 6.5pt, fill: muted)[#l],
    )),
  )
] else [
  #text(fill: faint)[No sales in this period.]
]

#pagebreak()

// =========================================================================
// PAYMENTS & CATEGORIES
// =========================================================================
#section-title[Payment mix]

#let pm = data.at("paymentMethods", default: ())
#if pm.len() > 0 {
  let cols = (c-retail, c-repair, c-print, faint)
  share-bar(
    pm.enumerate().map(((i, m)) => (
      m.at("label", default: "—"),
      m.at("amountCents", default: 0) / 100,
      cols.at(calc.rem(i, cols.len())),
    )),
    fmt: v => format-money(int(v * 100)),
  )
} else [ #text(fill: faint)[No payments recorded.] ]

#v(12pt)
#section-title[Sales by category]

#let cats = data.at("salesByCategory", default: ())
#if cats.len() > 0 {
  hbar-chart(
    cats.map(c => (c.at("name", default: "—"), c.at("revenueCents", default: 0) / 100)),
    color: c-retail,
    fmt: v => format-money(int(v * 100)),
  )
  v(6pt)
  table(
    columns: (1fr, auto, auto, auto),
    align: (left, right, right, right),
    stroke: (x, y) => (bottom: 0.4pt + border),
    inset: (x: 5pt, y: 4pt),
    fill: (x, y) => if y == 0 { panel } else { white },
    table-header([Category], [Units], [Revenue], [Gross profit]),
    ..cats
      .map(c => (
        text(size: 8pt)[#c.at("name", default: "—")],
        text(size: 8pt)[#c.at("unitsSold", default: 0)],
        text(size: 8pt)[#format-money(c.at("revenueCents", default: 0))],
        text(size: 8pt, weight: 600)[#format-money(c.at("grossProfitCents", default: 0))],
      ))
      .flatten(),
  )
} else [ #text(fill: faint)[No retail sales in this period.] ]

#pagebreak()

// =========================================================================
// PRODUCTS & CUSTOMERS
// =========================================================================
#section-title[Top products]

#let tp = data.at("topProducts", default: ())
#if tp.len() > 0 {
  table(
    columns: (auto, 1fr, auto, auto),
    align: (center, left, right, right),
    stroke: (x, y) => (bottom: 0.4pt + border),
    inset: (x: 5pt, y: 4pt),
    fill: (x, y) => if y == 0 { panel } else { white },
    table-header([\#], [Product], [Units sold], [Revenue]),
    ..tp
      .enumerate()
      .map(((i, p)) => (
        text(size: 8pt, fill: faint)[#(i + 1)],
        text(size: 8pt)[#p.at("name", default: "—")],
        text(size: 8pt)[#p.at("unitsSold", default: 0)],
        text(size: 8pt, weight: 600)[#format-money(p.at("revenueCents", default: 0))],
      ))
      .flatten(),
  )
} else [ #text(fill: faint)[No product sales in this period.] ]

#v(12pt)
#section-title[Top customers]

#let tc = data.at("topCustomers", default: ())
#if tc.len() > 0 {
  table(
    columns: (1fr, auto, auto, auto, auto),
    align: (left, right, right, right, right),
    stroke: (x, y) => (bottom: 0.4pt + border),
    inset: (x: 5pt, y: 4pt),
    fill: (x, y) => if y == 0 { panel } else { white },
    table-header([Customer], [Invoices], [Revenue], [Gross profit], [Outstanding]),
    ..tc
      .map(c => (
        text(size: 8pt)[#c.at("name", default: "—")],
        text(size: 8pt)[#c.at("invoiceCount", default: 0)],
        text(size: 8pt)[#format-money(c.at("revenueCents", default: 0))],
        text(size: 8pt, weight: 600)[#format-money(c.at("grossProfitCents", default: 0))],
        {
          let o = c.at("outstandingCents", default: 0)
          text(size: 8pt, fill: if o > 0 { bad } else { faint })[#format-money(o)]
        },
      ))
      .flatten(),
  )
} else [ #text(fill: faint)[No customer sales in this period.] ]

#pagebreak()

// =========================================================================
// STAFF & PATTERNS
// =========================================================================
#section-title[Cashier performance]

#let cp = data.at("cashierPerformance", default: ())
#if cp.len() > 0 {
  table(
    columns: (1fr, auto, auto, auto, auto),
    align: (left, right, right, right, right),
    stroke: (x, y) => (bottom: 0.4pt + border),
    inset: (x: 5pt, y: 4pt),
    fill: (x, y) => if y == 0 { panel } else { white },
    table-header([Cashier], [Invoices], [Revenue], [Discounts], [Gross profit]),
    ..cp
      .map(c => (
        text(size: 8pt)[#c.at("name", default: "—")],
        text(size: 8pt)[#c.at("invoiceCount", default: 0)],
        text(size: 8pt)[#format-money(c.at("revenueCents", default: 0))],
        text(size: 8pt)[#format-money(c.at("discountGivenCents", default: 0))],
        text(size: 8pt, weight: 600)[#format-money(c.at("grossProfitCents", default: 0))],
      ))
      .flatten(),
  )
} else [ #text(fill: faint)[No staff sales in this period.] ]

#v(12pt)
#section-title[Busiest days]

#let wp = data.at("weekdayPattern", default: ())
#if wp.len() == 7 {
  column-chart(
    wp.map(rupees),
    ("Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"),
    color: primary,
    height: 76pt,
    fmt: v => format-money(int(v * 100)),
  )
} else [ #text(fill: faint)[Not enough data for a weekday pattern.] ]

#pagebreak()

// =========================================================================
// RECEIVABLES, REFUNDS & INVENTORY
// =========================================================================
#section-title[Receivables aging]

#let ag = data.at("receivablesAging", default: ())
#if ag.len() > 0 {
  hbar-chart(
    ag.map(b => (b.at("label", default: "—"), b.at("amountCents", default: 0) / 100)),
    color: c-cogs,
    fmt: v => format-money(int(v * 100)),
  )
} else [ #text(fill: faint)[No open credit balances.] ]

#v(12pt)
#section-title[Refunds & returns]

#let rf = data.at("refunds", default: (:))
#grid(
  columns: (1fr, 1fr, 1fr),
  gutter: 8pt,
  ..(
    ("Credit notes", str(rf.at("creditNoteCount", default: 0))),
    ("Net refunded", format-money(rf.at("netRefundCents", default: 0))),
    ("Paid in cash", format-money(rf.at("refundCashCents", default: 0))),
  ).map(((l, val)) => block(width: 100%, inset: 8pt, radius: 4pt, stroke: 0.6pt + border, {
    text(size: 7pt, weight: "bold", fill: muted)[#upper(l)]
    v(2pt)
    text(size: 12pt, weight: 800)[#val]
  })),
)
#v(6pt)
#let rr = rf.at("byReason", default: ())
#if rr.len() > 0 {
  table(
    columns: (1fr, auto, auto),
    align: (left, right, right),
    stroke: (x, y) => (bottom: 0.4pt + border),
    inset: (x: 5pt, y: 4pt),
    fill: (x, y) => if y == 0 { panel } else { white },
    table-header([Reason], [Items], [Amount]),
    ..rr
      .map(r => (
        text(size: 8pt)[#humanize-reason(r.at("reason", default: "other"))],
        text(size: 8pt)[#r.at("count", default: 0)],
        text(size: 8pt, weight: 600)[#format-money(r.at("amountCents", default: 0))],
      ))
      .flatten(),
  )
}

#v(12pt)
#section-title[Inventory valuation]

#let iv = data.at("inventoryValuation", default: (:))
#grid(
  columns: (1fr, 1fr, 1fr, 1fr),
  gutter: 6pt,
  ..(
    ("At cost", format-money(iv.at("totalCostValuationCents", default: 0))),
    ("At retail", format-money(iv.at("totalRetailValuationCents", default: 0))),
    ("Potential profit", format-money(iv.at("potentialGrossProfitCents", default: 0))),
    ("Low / out of stock", str(iv.at("lowStockProductsCount", default: 0)) + " / " + str(iv.at("outOfStockProductsCount", default: 0))),
  ).map(((l, val)) => block(width: 100%, inset: 8pt, radius: 4pt, stroke: 0.6pt + border, {
    text(size: 6.5pt, weight: "bold", fill: muted)[#upper(l)]
    v(2pt)
    text(size: 10pt, weight: 800)[#val]
  })),
)
#v(6pt)
#let ivc = iv.at("categories", default: ())
#if ivc.len() > 0 {
  table(
    columns: (1fr, auto, auto),
    align: (left, right, right),
    stroke: (x, y) => (bottom: 0.4pt + border),
    inset: (x: 5pt, y: 4pt),
    fill: (x, y) => if y == 0 { panel } else { white },
    table-header([Category], [Cost value], [Retail value]),
    ..ivc
      .map(c => (
        text(size: 8pt)[#c.at("name", default: "—")],
        text(size: 8pt)[#format-money(c.at("costValuationCents", default: 0))],
        text(size: 8pt, weight: 600)[#format-money(c.at("retailValuationCents", default: 0))],
      ))
      .flatten(),
  )
}
