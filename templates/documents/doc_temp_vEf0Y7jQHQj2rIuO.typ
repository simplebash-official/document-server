// A4 Invoice — plain example. Input contract: the sibling .schema.json
// (worked example in the sibling .json).
//
// `logoUrl` (http(s) URL or data:image URI) is resolved by the server, which
// sets `logo` to a local image filename before compiling; leave `logo` unset
// in the payload. `logoWidth` is in pt.
#import "../lib/money.typ": format-money

// Defaults keep a partial payload compiling; the schema still guards real calls.
#let data = (invoiceNumber: "", formattedDate: "", formattedTime: "", cashierName: "", status: "", isCredit: false, copyDesignation: "", isDuplicate: false, paymentMethod: "", tenderedAmountCents: 0, items: (), subtotalCents: 0, discountCents: 0, totalCents: 0, amountInWords: "", showBankDetails: false, shopIsVatRegistered: false, shopAddressLines: ()) + sys.inputs
// Optional field, with "" treated as absent.
#let opt(key) = { let v = data.at(key, default: none); if v == "" { none } else { v } }

#set document(title: "Invoice " + data.invoiceNumber)
#set page(paper: "a4", margin: 18mm, footer: align(center, text(size: 8pt, data.copyDesignation)))
#set text(font: "Noto Sans", size: 9.5pt)

#let logo = opt("logo")

#grid(
  columns: (auto, 1fr, auto),
  column-gutter: 8pt,
  if logo != none { image(logo, width: data.at("logoWidth", default: 46) * 1pt) },
  [
    #text(size: 13pt, strong(data.at("shopTradingName", default: "")))
    #if opt("shopLegalName") != none [ \ #data.shopLegalName]
    #for line in data.shopAddressLines [ \ #line]
    #if opt("shopPrimaryPhone") != none [ \ Tel: #data.shopPrimaryPhone]
    #if opt("shopEmail") != none [ \ #data.shopEmail]
    #if opt("shopBusinessRegNo") != none [ \ Reg No: #data.shopBusinessRegNo]
    #if data.shopIsVatRegistered and opt("shopVatNo") != none [ \ VAT No: #data.shopVatNo]
  ],
  align(right)[
    #text(size: 16pt, strong(if data.shopIsVatRegistered [TAX INVOICE] else [INVOICE])) \
    #if data.isDuplicate [DUPLICATE \ ]
    No: #data.invoiceNumber \
    Date: #data.formattedDate #data.formattedTime \
    Status: #data.status \
    Cashier: #data.cashierName
  ],
)
#line(length: 100%)

#if opt("customerName") != none [
  *Bill to:* #data.customerName
  #if opt("customerPhone") != none [ \ #data.customerPhone]
  #if opt("customerAddress") != none [ \ #data.customerAddress]
]

#v(6pt)
#table(
  columns: (1fr, auto, auto, auto, auto),
  stroke: 0.5pt + gray,
  table.header[*Item*][*Qty*][*Unit*][*Discount*][*Total*],
  ..data.items.map(item => (
    [#item.name #if item.at("sku", default: none) != none [\ #text(size: 8pt, item.sku)]],
    str(item.quantity),
    format-money(item.unitPriceCents),
    format-money(item.discountCents),
    format-money(item.totalCents),
  )).flatten(),
)

#align(right)[
  Subtotal: #format-money(data.subtotalCents) \
  #if data.discountCents > 0 [Discount: -#format-money(data.discountCents) \ ]
  #if opt("taxCents") != none [
    Tax#if opt("vatRatePercent") != none [ (#data.vatRatePercent%)]: #format-money(data.taxCents) \
  ]
  *Total: #format-money(data.totalCents)* \
  #text(size: 8.5pt, data.amountInWords)
]

*Payment:* #data.paymentMethod
#if opt("cardLast4") != none [(card \*\*\*\* #data.cardLast4)]
#if opt("cardRef") != none [ref #data.cardRef]
#if data.tenderedAmountCents > 0 [ \ Tendered: #format-money(data.tenderedAmountCents)]
#if data.isCredit {
  if opt("amountPaidCents") != none [ \ Paid: #format-money(data.amountPaidCents)]
  if opt("balanceDueCents") != none [ \ *Balance due: #format-money(data.balanceDueCents)*]
  if opt("dueDate") != none [ \ Pay by: #data.dueDate]
}

#if data.showBankDetails [
  #v(6pt)
  *Bank details:* #data.at("bankName", default: "") #data.at("bankBranch", default: "") \
  #data.at("accountName", default: "") — #data.at("accountNumber", default: "")
]
#if opt("notes") != none [#v(6pt) *Notes:* #data.notes]
#if opt("warrantyText") != none [#v(6pt) #text(size: 8pt, data.warrantyText)]
