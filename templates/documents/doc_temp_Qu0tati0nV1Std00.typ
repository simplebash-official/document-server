// Quotation — plain example. Input contract: the sibling .schema.json
// (worked example in the sibling .json). A4 portrait. A priced offer, not a bill.
#import "../lib/money.typ": format-money

// Defaults keep a partial payload compiling; the schema still guards real calls.
#let data = (quotationNumber: "", formattedDate: "", validUntil: "", isCancelled: false, items: (), subtotalCents: 0, discountCents: 0, totalCents: 0, amountInWords: "", shopAddressLines: ()) + sys.inputs
// Optional field, with "" treated as absent.
#let opt(key) = { let v = data.at(key, default: none); if v == "" { none } else { v } }

#set document(title: "Quotation " + data.quotationNumber)
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
  ],
  align(right)[
    #text(size: 16pt, strong[QUOTATION]) \
    No: #data.quotationNumber \
    Date: #data.formattedDate \
    Valid until: #data.validUntil
    #if data.isCancelled [ \ #text(fill: red, strong[CANCELLED])]
  ],
)
#line(length: 100%)

#if opt("customerName") != none [
  *Prepared for:* #data.customerName
  #if opt("customerPhone") != none [ \ Phone: #data.customerPhone]
  #if opt("customerAddress") != none [ \ #data.customerAddress]
]

#v(6pt)
#table(
  columns: (1fr, auto, auto, auto, auto),
  stroke: 0.5pt + gray,
  table.header[*Item*][*Qty*][*Unit*][*Discount*][*Total*],
  ..data.items.map(item => (
    item.name,
    str(item.quantity),
    format-money(item.unitPriceCents),
    format-money(item.discountCents),
    format-money(item.totalCents),
  )).flatten(),
)

#align(right)[
  Subtotal: #format-money(data.subtotalCents) \
  #if data.discountCents > 0 [Discount: -#format-money(data.discountCents) \ ]
  *Total: #format-money(data.totalCents)*
]

#if opt("amountInWords") != none [#emph(data.amountInWords)]
#if opt("notes") != none [ \ *Notes:* #data.notes]
#if opt("terms") != none [ \ *Terms:* #data.terms]
#if opt("preparedBy") != none [ \ Prepared by: #data.preparedBy]
