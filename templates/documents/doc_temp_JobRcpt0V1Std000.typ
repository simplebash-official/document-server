// Job Receipt — plain example. Input contract: the sibling .schema.json (worked example in the
// sibling .json). The slip a customer gets when the shop receives a repair or a print job, on an
// A5 sheet or a 58/80 mm roll (`paper`). Labels are fixed English with Sinhala under them; the
// data prints as given. The QR code holds the ticket number, which the till scans.
#import "../lib/qrcode.typ": qrcode
#import "../lib/money.typ": format-money

// Defaults keep a partial payload compiling; the schema still guards real calls.
#let data = (paper: "a5", ticketNumber: "", jobKind: "repair", receivedOn: "", receivedAt: "", customerName: "", customerPhone: "", title: "", detail: "", serialNumber: "", itemLines: (), estimateCents: none, promisedReadyOn: none, isWarrantyRework: false, qrData: "", termsText: "", uncollectedAfterDays: none, shopTradingName: "", shopAddressLines: ()) + sys.inputs
#let opt(key) = { let v = data.at(key, default: none); if v == "" { none } else { v } }
#let thermal = data.paper != "a5"

#set document(title: "Job receipt " + data.ticketNumber)
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
  #text(size: 13pt, strong[JOB RECEIPT]) \
  #text(size: 10pt)[භාර ගත් රිසිට්පත] \
  #text(size: 11pt, strong(data.ticketNumber))
  #if data.isWarrantyRework [ \ #text(size: 9pt)[Warranty rework · No charge]]
]
#v(4pt)

#grid(
  columns: (auto, 1fr),
  column-gutter: 8pt,
  row-gutter: 6pt,
  ..field("Received", "භාර ගත් දිනය", [#data.receivedOn · #data.receivedAt]),
  ..field("Customer", "කස්ටමර්", [#data.customerName #if opt("customerPhone") != none [· #data.customerPhone]]),
  ..field(if data.jobKind == "repair" { "Item" } else { "Job" }, if data.jobKind == "repair" { "බඩුව" } else { "වැඩේ" }, data.title),
  ..if opt("serialNumber") != none { field("Serial / IMEI", "Serial / IMEI අංකය", raw(data.serialNumber)) },
  ..field(if data.jobKind == "repair" { "Problem" } else { "Details" }, if data.jobKind == "repair" { "ගැටලුව" } else { "විස්තර" }, data.detail),
  ..if data.itemLines.len() > 0 { field("Work planned", "කරන වැඩ", data.itemLines.join(", ")) },
  ..if data.estimateCents != none { field("Estimate", "ඇස්තමේන්තුව", format-money(data.estimateCents)) },
  ..if data.promisedReadyOn != none { field("Ready by", "ලෑස්ති වෙන දිනය", data.promisedReadyOn) },
)

#if opt("termsText") != none [
  #v(4pt)
  #lbl("Terms", "කොන්දේසි") \
  #text(size: 0.9em, data.termsText)
]
#if data.uncollectedAfterDays != none [
  #v(4pt)
  #text(size: 0.9em)[Items not collected within #data.uncollectedAfterDays days are no longer the shop's responsibility.]
]

#v(10pt)
#line(length: 60%, stroke: 0.5pt)
#text(size: 0.85em)[Customer signature · පාරිභෝගික අත්සන]

#v(6pt)
#align(center)[
  #qrcode(data.qrData, size: if thermal { 18mm } else { 22mm }) \
  #text(size: 0.85em)[Bring this receipt when you collect. \ බඩු ගන්න එනකොට මේ රිසිට්පත ගෙනෙන්න.]
]
