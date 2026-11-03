// Warranty Card — plain example. Input contract: the sibling .schema.json (worked example in
// the sibling .json). One warranty on an A5 sheet or a 58/80 mm roll (`paper`). Labels are fixed
// English with Sinhala under them; the data prints as given. The QR code holds the warranty
// number, which the shop's warranty lookup accepts.
#import "../lib/qrcode.typ": qrcode

// Defaults keep a partial payload compiling; the schema still guards real calls.
#let data = (paper: "a5", warrantyNumber: "", itemName: "", serialNumber: "", quantity: 1, coverageLabel: "", warrantyTypeLabel: "", startsOn: "", validUntil: none, isVoid: false, replacesWarrantyNumber: "", terms: "", generalPolicy: "", invoiceNumber: "", purchaseDate: "", customerName: "", customerPhone: "", shopTradingName: "", shopAddressLines: ()) + sys.inputs
#let opt(key) = { let v = data.at(key, default: none); if v == "" { none } else { v } }
#let thermal = data.paper != "a5"

#set document(title: "Warranty " + data.warrantyNumber)
// A set rule inside `if` would end with its block, so the page arguments are chosen instead.
#set page(..if thermal {
  (width: (if data.paper == "thermal-58" { 58 } else { 80 }) * 1mm, height: auto, margin: (x: 2mm, y: 4mm))
} else {
  (paper: "a5", margin: 12mm)
})
#set text(font: ("Noto Sans", "Noto Sans Sinhala"), size: if thermal { 8pt } else { 9.5pt })

// A label in English with its Sinhala under it.
#let lbl(en, si) = [#strong(en) \ #text(size: 0.8em, fill: luma(90), si)]
#let field(en, si, value) = (lbl(en, si), value)

#align(center)[
  #text(size: 12pt, strong(data.shopTradingName)) \
  #for line in data.shopAddressLines [#line \ ]
  #if opt("shopPrimaryPhone") != none [Tel: #data.shopPrimaryPhone]
]
#line(length: 100%)
#align(center)[
  #text(size: 13pt, strong[WARRANTY CARD]) \
  #text(size: 10pt)[වගකීම් කාඩ්පත] \
  #text(size: 11pt, strong(data.warrantyNumber))
  #if data.isVoid [ \ #text(size: 14pt, fill: red, strong[VOID · අවලංගුයි])]
]
#v(4pt)

#grid(
  columns: (auto, 1fr),
  column-gutter: 8pt,
  row-gutter: 6pt,
  ..field("Item", "බඩුව", [#data.itemName #if data.quantity > 1 [× #data.quantity]]),
  ..if opt("serialNumber") != none { field("Serial / IMEI", "Serial / IMEI අංකය", raw(data.serialNumber)) },
  ..if opt("customerName") != none { field("Customer", "කස්ටමර්", [#data.customerName #if opt("customerPhone") != none [· #data.customerPhone]]) },
  ..field("Invoice", "බිල", [#data.invoiceNumber · #data.purchaseDate]),
  ..field("Period", "කාලය", [#data.coverageLabel · #data.warrantyTypeLabel]),
  ..field("Valid from", "පටන් ගන්නා දිනය", data.startsOn),
  ..field("Valid until", "වලංගු වෙන අවසාන දිනය", if data.validUntil == none [No end date] else [#strong(data.validUntil)]),
  ..if opt("replacesWarrantyNumber") != none { field("Replaces", "වෙනුවට දුන්නේ", data.replacesWarrantyNumber) },
)

#if opt("terms") != none [
  #v(4pt)
  #lbl("Terms", "කොන්දේසි") \
  #text(size: 0.9em, data.terms)
]
#if opt("generalPolicy") != none [
  #v(4pt)
  #lbl("Warranty policy", "වගකීම් ප්‍රතිපත්තිය") \
  #text(size: 0.9em, data.generalPolicy)
]

#v(6pt)
#align(center)[
  #qrcode(data.warrantyNumber, size: if thermal { 18mm } else { 22mm }) \
  #text(size: 0.85em)[Bring this card with the item. \ මේ කාඩ්පත බඩුවත් එක්ක අරන් එන්න.]
]
