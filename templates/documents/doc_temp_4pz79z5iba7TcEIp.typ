// Thermal Receipt — plain example. Input contract: the sibling .schema.json
// (worked example in the sibling .json). 58 mm or 80 mm roll via paperWidthMm.
#import "../lib/codes.typ": barcode
#import "../lib/money.typ": format-money

// Defaults keep a partial payload compiling; the schema still guards real calls.
#let data = (paperWidthMm: 80, invoiceNumber: "", formattedDate: "", formattedTime: "", cashierName: "", items: (), subtotalCents: 0, discountCents: 0, totalCents: 0, paymentMethod: "", isCredit: false, shopTradingName: "", shopLegalName: "", shopAddressLines: ()) + sys.inputs
// Optional field, with "" treated as absent.
#let opt(key) = { let v = data.at(key, default: none); if v == "" { none } else { v } }

#set document(title: "Receipt " + data.invoiceNumber)
#set page(width: data.at("paperWidthMm", default: 80) * 1mm, height: auto, margin: (x: 2mm, y: 4mm))
#set text(font: "Noto Sans", size: 8.5pt)

#let row(left, right) = grid(columns: (1fr, auto), left, right)

#align(center)[
  #text(size: 11pt, strong(data.shopTradingName)) \
  #data.shopLegalName \
  #for line in data.shopAddressLines [#line \ ]
  #if opt("shopPrimaryPhone") != none [Tel: #data.shopPrimaryPhone]
]
#line(length: 100%)
#row[Invoice #data.invoiceNumber][#data.formattedDate #data.formattedTime]
Cashier: #data.cashierName
#if opt("customerName") != none [ \ Customer: #data.customerName]
#if opt("customerPhone") != none [ \ Phone: #data.customerPhone]
#line(length: 100%)

#for item in data.items [
  #item.name \
  #row[#item.quantity x #format-money(item.unitPriceCents)][#format-money(item.totalCents)]
  #if item.discountCents > 0 [#row[Discount][-#format-money(item.discountCents)]]
]
#line(length: 100%)
#row[Subtotal][#format-money(data.subtotalCents)]
#if data.discountCents > 0 [#row[Discount][-#format-money(data.discountCents)]]
#if opt("taxCents") != none and data.taxCents > 0 [#row[Tax][#format-money(data.taxCents)]]
#row[*TOTAL*][*#format-money(data.totalCents)*]
#line(length: 100%)

#row[Payment][#upper(data.paymentMethod)]
#if opt("tenderedAmountCents") != none [#row[Tendered][#format-money(data.tenderedAmountCents)]]
#if opt("changeDueCents") != none [#row[Change][#format-money(data.changeDueCents)]]
#if opt("cardLast4") != none [#row[Card][\*\*\*\* #data.cardLast4]]
#for split in data.at("splitPayments", default: ()) [
  #row[#upper(split.method)][#format-money(split.amountCents)]
]
#if data.isCredit {
  if opt("amountPaidCents") != none [#row[Paid][#format-money(data.amountPaidCents)]]
  if opt("balanceDueCents") != none [#row[*Balance due*][*#format-money(data.balanceDueCents)*]]
  if opt("dueDate") != none [#row[Pay by][#data.dueDate]]
}

#v(4pt)
#align(center)[
  #barcode(data.invoiceNumber, height: 8mm, show-text: false)
  #if opt("warrantyText") != none [#text(size: 7pt, data.warrantyText) \ ]
  #data.at("footerText", default: "Thank you!")
]
