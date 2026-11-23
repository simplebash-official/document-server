// Job Receipt — plain example. Input contract: the sibling .schema.json (worked example in the
// sibling .json). The slip handed over when the shop receives a repair or print job, on an A5
// sheet or a 58/80 mm roll (`paper`). Labels are fixed English with Sinhala under them; the data
// prints as given. The QR code holds the ticket number, which the till's scan box accepts.
#import "../lib/qrcode.typ": qrcode

// Defaults keep a partial payload compiling; the schema still guards real calls.
#let data = (paper: "a5", ticketNumber: "", jobKind: "repair", receivedOn: "", receivedAt: "", customerName: "", customerPhone: "", title: "", detail: "", serialNumber: "", itemLines: (), estimateCents: none, promisedReadyOn: none, isWarrantyRework: false, qrData: "", termsText: "", uncollectedAfterDays: none, shopTradingName: "", shopAddressLines: ()) + sys.inputs
#let opt(key) = { let v = data.at(key, default: none); if v == "" { none } else { v } }
#let thermal = data.paper != "a5"
#let rupees(cents) = {
  let whole = calc.quo(cents, 100)
  let digits = str(whole)
  let grouped = ""
  for (i, c) in digits.clusters().enumerate() {
    if i > 0 and calc.rem(digits.len() - i, 3) == 0 { grouped += "," }
    grouped += c
  }
  "Rs. " + grouped + "." + (if calc.rem(cents, 100) < 10 { "0" } else { "" }) + str(calc.rem(cents, 100))
}

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
  #text(size: 10pt)[රැගත් බව සනාථ කිරීමේ රිසිට්පත] \
  #text(size: 11pt, strong(data.ticketNumber)) \
  #data.receivedOn · #data.receivedAt
]
#v(4pt)

#let estimate = if data.isWarrantyRework [No charge · ගාස්තුවක් නැත] else if data.estimateCents == none [To be quoted · තවම ගණන් බලා නැත] else [#strong(rupees(data.estimateCents))]

#grid(
  columns: (auto, 1fr),
  column-gutter: 8pt,
  row-gutter: 6pt,
  ..field("Customer", "කස්ටමර්", [#data.customerName #if opt("customerPhone") != none [· #data.customerPhone]]),
  ..field(if data.jobKind == "repair" { "Device" } else { "Print" }, if data.jobKind == "repair" { "උපාංගය" } else { "මුද්‍රණය" }, data.title),
  ..if opt("serialNumber") != none { field("Serial / IMEI", "Serial / IMEI අංකය", raw(data.serialNumber)) },
  ..if opt("detail") != none { field(if data.jobKind == "repair" { "Issue" } else { "Order" }, if data.jobKind == "repair" { "ගැටලුව" } else { "ඇණවුම" }, data.detail) },
  ..if data.itemLines.len() > 0 { field("Planned work", "කරන වැඩ", [#for item in data.itemLines [- #item \ ]]) },
  ..field("Estimate", "ඇස්තමේන්තුව", estimate),
  ..if data.promisedReadyOn != none { field("Ready by", "සූදානම් වන දිනය", data.promisedReadyOn) },
)

#if opt("termsText") != none [
  #v(4pt)
  #lbl("Terms", "කොන්දේසි") \
  #text(size: 0.9em, data.termsText)
]
#if data.uncollectedAfterDays != none [
  #v(4pt)
  #text(size: 0.9em)[*Please collect within #data.uncollectedAfterDays days.* Items left longer may be disposed of. \
  #text(size: 0.9em, fill: luma(90))[දින #data.uncollectedAfterDays ක් ඇතුළත රැගෙන යන්න.]]
]

#v(6pt)
#align(center)[
  #qrcode(data.qrData, size: if thermal { 18mm } else { 22mm }) \
  #text(size: 0.85em)[Bring this slip when you collect. \ රැගෙන යන විට මේ රිසිට්පත ගෙනෙන්න.]
]
