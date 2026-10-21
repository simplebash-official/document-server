// A4 Z-Report (End-of-Session Summary) — plain example.
// Input contract: the sibling .schema.json (worked example in the sibling .json).
#import "../lib/money.typ": format-money

#let data = (
  sessionNumber: "",
  terminalId: "",
  cashierName: "",
  status: "closed",
  openedAt: "",
  closedAt: "",
  grossSalesCents: 0,
  returnsCents: 0,
  netSalesCents: 0,
  discountCents: 0,
  totalInvoicesCount: 0,
  paymentMethods: (),
  cashReconciliation: (
    openingFloatCents: 0,
    cashSalesCents: 0,
    payInCents: 0,
    payOutCents: 0,
    cashRefundsCents: 0,
    ownerWithdrawalsCents: 0,
    cashDropsCents: 0,
    expectedCashCents: 0,
    countedCashCents: 0,
    varianceCents: 0,
    closingFloatCents: 0,
    safeDepositCents: 0,
  ),
  cashMovements: (),
  shopTradingName: "",
  shopAddressLines: (),
  generatedAt: "",
) + sys.inputs

#let opt(key) = { let v = data.at(key, default: none); if v == "" { none } else { v } }

#set document(title: "Z-Report " + data.sessionNumber)
#set page(paper: "a4", margin: 15mm, footer: align(center, text(size: 8pt, fill: gray)[Z-Report #data.sessionNumber · Generated: #data.generatedAt]))
#set text(font: "Noto Sans", size: 9pt)

#let logo = opt("logo")

#grid(
  columns: (auto, 1fr, auto),
  column-gutter: 10pt,
  if logo != none { image(logo, width: data.at("logoWidth", default: 46) * 1pt) },
  [
    #text(size: 13pt, strong(data.shopTradingName))
    #if opt("shopLegalName") != none [ \ #data.shopLegalName]
    #for line in data.shopAddressLines [ \ #line]
    #if opt("shopPrimaryPhone") != none [ \ Tel: #data.shopPrimaryPhone]
    #if opt("shopEmail") != none [ \ #data.shopEmail]
  ],
  align(right)[
    #text(size: 16pt, strong[REGISTER Z-REPORT]) \
    #text(size: 9pt, fill: gray)[End of Session Summary] \
    *Session:* #data.sessionNumber \
    *Terminal:* #data.terminalId \
    *Status:* #upper(data.status) \
    *Cashier:* #data.cashierName \
    *Opened:* #data.openedAt \
    *Closed:* #data.closedAt
  ],
)
#line(length: 100%, stroke: 1pt + black)
#v(6pt)

#text(size: 11pt, strong[1. Financial Summary])
#v(2pt)
#table(
  columns: (1fr, auto),
  stroke: 0.5pt + gray,
  [Gross Sales], format-money(data.grossSalesCents),
  [Returns / Credit Notes], [-#format-money(data.returnsCents)],
  [Discounts Given], [-#format-money(data.discountCents)],
  table.hline(stroke: 1pt + black),
  [*Net Sales*], [*#format-money(data.netSalesCents)*],
  [Total Invoices Count], str(data.totalInvoicesCount),
)
#v(8pt)

#text(size: 11pt, strong[2. Payment Methods])
#v(2pt)
#table(
  columns: (1fr, auto, auto),
  stroke: 0.5pt + gray,
  table.header([*Payment Method*], [*Count*], [*Total Amount*]),
  ..data.paymentMethods.map(pm => (
    upper(pm.method),
    str(pm.count),
    format-money(pm.totalCents),
  )).flatten(),
)
#v(8pt)

#text(size: 11pt, strong[3. Cash Drawer Reconciliation])
#v(2pt)
#let recon = data.cashReconciliation
#table(
  columns: (1fr, auto),
  stroke: 0.5pt + gray,
  [Opening Float], format-money(recon.openingFloatCents),
  [Cash Sales (+)], [+#format-money(recon.cashSalesCents)],
  [Pay-Ins (+)], [+#format-money(recon.payInCents)],
  [Pay-Outs (Expenses) (-)], [-#format-money(recon.payOutCents)],
  [Cash Refunds (-)], [-#format-money(recon.cashRefundsCents)],
  [Owner Withdrawals (-)], [-#format-money(recon.ownerWithdrawalsCents)],
  [Cash Drops to Safe (-)], [-#format-money(recon.cashDropsCents)],
  table.hline(stroke: 1pt + black),
  [*Expected Drawer Cash*], [*#format-money(recon.expectedCashCents)*],
  [*Actual Counted Cash*], [*#format-money(recon.countedCashCents)*],
  [*Cash Variance*], [*#format-money(recon.varianceCents)*],
  table.hline(stroke: 0.5pt + gray),
  [Retained Closing Float], format-money(recon.closingFloatCents),
  [Safe Deposit / Bank], format-money(recon.safeDepositCents),
)

#if data.cashMovements.len() > 0 [
  #v(8pt)
  #text(size: 11pt, strong[4. Cash Movements Audit Trail])
  #v(2pt)
  #table(
    columns: (auto, auto, auto, 1fr, auto),
    stroke: 0.5pt + gray,
    table.header([*Ref*], [*Type*], [*Amount*], [*Reason*], [*Time*]),
    ..data.cashMovements.map(cm => (
      cm.movementNumber,
      cm.movementType,
      format-money(cm.amountCents),
      cm.reason,
      cm.createdAt,
    )).flatten(),
  )
]
