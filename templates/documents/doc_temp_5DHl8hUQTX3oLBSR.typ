// Credit Note — plain example. Input contract: the sibling .schema.json
// (worked example in the sibling .json). A4 portrait.
#import "../lib/money.typ": format-money

// Defaults keep a partial payload compiling; the schema still guards real calls.
#let data = (creditNoteNumber: "", formattedDate: "", cashierName: "", noReceipt: false, isManagerOverride: false, items: (), refundCashCents: 0, balanceReductionCents: 0, refundBreakdown: (), shopIsVatRegistered: false, shopAddressLines: ()) + sys.inputs
// Optional field, with "" treated as absent.
#let opt(key) = { let v = data.at(key, default: none); if v == "" { none } else { v } }

#set document(title: "Credit Note " + data.creditNoteNumber)
#set page(paper: "a4", margin: 18mm)
#set text(font: "Noto Sans", size: 9.5pt)

#grid(
  columns: (1fr, auto),
  [
    #text(size: 13pt, strong(data.at("shopTradingName", default: "")))
    #if opt("shopLegalName") != none [ \ #data.shopLegalName]
    #for line in data.shopAddressLines [ \ #line]
    #if opt("shopPrimaryPhone") != none [ \ Tel: #data.shopPrimaryPhone]
    #if opt("shopEmail") != none [ \ #data.shopEmail]
    #if data.shopIsVatRegistered and opt("shopVatNo") != none [ \ VAT No: #data.shopVatNo]
  ],
  align(right)[
    #text(size: 16pt, strong[CREDIT NOTE]) \
    No: #data.creditNoteNumber \
    Date: #data.formattedDate \
    Cashier: #data.cashierName
  ],
)
#line(length: 100%)

#if data.noReceipt [Returned without a receipt. ]
#if opt("originalInvoiceNumber") != none [Original invoice: #data.originalInvoiceNumber. ]
#if opt("exchangeReference") != none [Exchange: #data.exchangeReference. ]
#if data.isManagerOverride [Approved by manager override.]
#if opt("customerName") != none [ \ Customer: #data.customerName #data.at("customerPhone", default: "")]

#v(6pt)
#table(
  columns: (1fr, auto, auto, auto, auto),
  stroke: 0.5pt + gray,
  table.header[*Item*][*Condition*][*Qty*][*Unit*][*Total*],
  ..data.items.map(item => (
    [#item.name #if item.at("serialNumber", default: "") != "" [(S/N #item.serialNumber)]],
    item.condition,
    str(item.quantity),
    format-money(item.unitPriceCents),
    format-money(item.totalCents),
  )).flatten(),
)

#align(right)[
  #for part in data.refundBreakdown [Refund (#part.method): #format-money(part.amountCents) \ ]
  Cash refunded: #format-money(data.refundCashCents) \
  Balance reduced: #format-money(data.balanceReductionCents)
]

#if opt("notes") != none [*Notes:* #data.notes]
