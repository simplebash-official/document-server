// Analytics Report — plain example. Input contract: the sibling .schema.json
// (worked example in the sibling .json). Only generatedAt, periodLabel,
// granularityLabel, kpis and timeseries are required; every other section is
// skipped when absent or empty. Money arrives as integer cents.
#import "../lib/money.typ": format-money
#import "../lib/charts.typ": column-chart, line-chart, hbar-chart, share-bar

#let data = sys.inputs
#let ts = data.timeseries
#let rupees(cents) = cents / 100
#let money(v) = format-money(int(v * 100))
#let palette = (rgb("#3D4EAC"), rgb("#2E9E6A"), rgb("#E0A030"), rgb("#C04848"), rgb("#7A5CC0"))
#let has(key) = data.at(key, default: none) not in (none, (), (:))

#set document(title: "Analytics Report " + data.periodLabel)
#set page(
  paper: "a4",
  margin: 16mm,
  footer: context align(center, text(size: 7pt)[Page #counter(page).display() — generated #data.generatedAt]),
)
#set text(font: "Noto Sans", size: 9pt)
#show heading: set block(above: 14pt, below: 6pt)

#let section(title, body) = block(breakable: true, heading(title) + body)
#let money-table(columns, header, rows) = table(
  columns: columns,
  stroke: 0.5pt + gray,
  table.header(..header.map(h => strong(h))),
  ..rows.flatten(),
)

#text(size: 16pt, strong(data.at("shopName", default: "Analytics Report"))) \
#for line in data.at("shopAddressLines", default: ()) [#line \ ]
#data.periodLabel · #data.granularityLabel
#if data.at("comparisonLabel", default: none) != none [ · #data.comparisonLabel]

#section("Key figures", grid(
  columns: (1fr, 1fr, 1fr, 1fr),
  gutter: 6pt,
  ..data.kpis.map(k => rect(width: 100%, stroke: 0.5pt + gray, inset: 6pt)[
    #text(size: 7.5pt, k.label) \
    #text(size: 11pt, strong(k.value))
    #if k.at("deltaLabel", default: none) != none [ \ #text(size: 7pt, k.deltaLabel)]
  ]),
))

#if ts.labels.len() > 0 {
  section("Revenue", column-chart(ts.revenueCents.map(rupees), ts.labels, fmt: v => money(v)))
  section("Revenue vs gross profit", line-chart(ts.labels, (
    (values: ts.revenueCents.map(rupees), color: palette.at(0)),
    (values: ts.grossProfitCents.map(rupees), color: palette.at(1)),
  )))
}

#if has("paymentMethods") {
  section("Payment methods", share-bar(
    data.paymentMethods.enumerate().map(((i, p)) => (p.label, rupees(p.amountCents), palette.at(calc.rem(i, palette.len())))),
    fmt: v => money(v),
  ))
}

#if has("salesByCategory") {
  section("Sales by category", money-table(
    (1fr, auto, auto, auto),
    ("Category", "Units", "Revenue", "Gross profit"),
    data.salesByCategory.map(c => (c.name, str(c.unitsSold), format-money(c.revenueCents), format-money(c.grossProfitCents))),
  ))
}

#if has("topProducts") {
  section("Top products", hbar-chart(
    data.topProducts.map(p => (p.name, rupees(p.revenueCents))),
    fmt: v => money(v),
  ))
}

#if has("topCustomers") {
  section("Top customers", money-table(
    (1fr, auto, auto, auto, auto),
    ("Customer", "Invoices", "Revenue", "Gross profit", "Outstanding"),
    data.topCustomers.map(c => (
      c.name, str(c.invoiceCount), format-money(c.revenueCents),
      format-money(c.at("grossProfitCents", default: 0)), format-money(c.at("outstandingCents", default: 0)),
    )),
  ))
}

#if has("cashierPerformance") {
  section("Cashier performance", money-table(
    (1fr, auto, auto, auto, auto),
    ("Cashier", "Invoices", "Revenue", "Discounts", "Gross profit"),
    data.cashierPerformance.map(c => (
      c.name, str(c.invoiceCount), format-money(c.revenueCents),
      format-money(c.at("discountGivenCents", default: 0)), format-money(c.at("grossProfitCents", default: 0)),
    )),
  ))
}

#if has("receivablesAging") {
  section("Receivables aging", money-table(
    (1fr, auto, auto),
    ("Bucket", "Invoices", "Amount"),
    data.receivablesAging.map(r => (r.label, str(r.invoiceCount), format-money(r.amountCents))),
  ))
}

#if has("weekdayPattern") {
  section("Weekday pattern", column-chart(
    data.weekdayPattern.map(rupees),
    ("Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun").slice(0, calc.min(7, data.weekdayPattern.len())),
    fmt: v => money(v),
  ))
}

#if has("refunds") {
  let r = data.refunds
  section("Refunds", [
    #r.at("creditNoteCount", default: 0) credit notes, net #format-money(r.at("netRefundCents", default: 0)),
    cash #format-money(r.at("refundCashCents", default: 0)).
    #if r.at("byReason", default: ()).len() > 0 {
      money-table(
        (1fr, auto, auto),
        ("Reason", "Count", "Amount"),
        r.byReason.map(b => (b.reason.replace("_", " "), str(b.count), format-money(b.amountCents))),
      )
    }
  ])
}

#if has("inventoryValuation") {
  let inv = data.inventoryValuation
  section("Inventory valuation", [
    Cost #format-money(inv.at("totalCostValuationCents", default: 0)) ·
    Retail #format-money(inv.at("totalRetailValuationCents", default: 0)) ·
    Potential profit #format-money(inv.at("potentialGrossProfitCents", default: 0)) \
    Low stock: #inv.at("lowStockProductsCount", default: 0) · Out of stock: #inv.at("outOfStockProductsCount", default: 0)
    #if inv.at("categories", default: ()).len() > 0 {
      money-table(
        (1fr, auto, auto),
        ("Category", "Cost", "Retail"),
        inv.categories.map(c => (c.name, format-money(c.costValuationCents), format-money(c.retailValuationCents))),
      )
    }
  ])
}
