// Job Receipt — plain example. Input contract: the sibling .schema.json (worked example in the
// sibling .json). The slip a customer gets when the shop receives one or more repair or print
// jobs from them, on an A5 sheet or a 58/80 mm roll (`paper`). Labels are fixed English with
// Sinhala under them; the data prints as given. Each job's QR code holds its ticket number,
// which the till scans.
#import "../lib/qrcode.typ": qrcode
#import "../lib/money.typ": format-money

// Defaults keep a partial payload compiling; the schema still guards real calls.
#let data = (paper: "a5", customerName: "", customerPhone: "", jobs: (), estimateTotalCents: none, termsText: "", uncollectedAfterDays: none, shopTradingName: "", shopAddressLines: ()) + sys.inputs
#let opt(key) = { let v = data.at(key, default: none); if v == "" { none } else { v } }
#let thermal = data.paper != "a5"
#let jobs = data.jobs
#let single = jobs.len() == 1

#set document(title: "Job receipt " + jobs.map(job => job.ticketNumber).join(", "))
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
#let fields(rows) = grid(columns: (auto, 1fr), column-gutter: 8pt, row-gutter: 6pt, ..rows)

#let customer = field("Customer", "කස්ටමර්", [#data.customerName #if opt("customerPhone") != none [· #data.customerPhone]])
#let job-rows(job) = {
  let repair = job.jobKind == "repair"
  (
    ..field("Received", "භාර ගත් දිනය", [#job.receivedOn · #job.receivedAt]),
    ..if single { customer },
    ..field(if repair { "Item" } else { "Job" }, if repair { "බඩුව" } else { "වැඩේ" }, job.title),
    ..if job.serialNumber != "" { field("Serial / IMEI", "Serial / IMEI අංකය", raw(job.serialNumber)) },
    ..field(if repair { "Problem" } else { "Details" }, if repair { "ගැටලුව" } else { "විස්තර" }, job.detail),
    ..if job.itemLines.len() > 0 { field("Work planned", "කරන වැඩ", job.itemLines.join(", ")) },
    ..if job.estimateCents != none { field("Estimate", "ඇස්තමේන්තුව", format-money(job.estimateCents)) },
    ..if job.promisedReadyOn != none { field("Ready by", "ලෑස්ති වෙන දිනය", job.promisedReadyOn) },
  )
}
#let job-code(job, size) = qrcode(job.qrData, size: size)
#let rework-note = text(size: 9pt)[Warranty rework · No charge]

#align(center)[
  #text(size: 12pt, strong(data.shopTradingName)) \
  #for line in data.shopAddressLines [#line \ ]
  #if opt("shopPrimaryPhone") != none [Tel: #data.shopPrimaryPhone]
]
#line(length: 100%)
#align(center)[
  #text(size: 13pt, strong[JOB RECEIPT]) \
  #text(size: 10pt)[භාර ගත් රිසිට්පත]
  #if single [
    \ #text(size: 11pt, strong(jobs.first().ticketNumber))
    #if jobs.first().isWarrantyRework [ \ #rework-note]
  ]
]
#v(4pt)

#if single {
  fields(job-rows(jobs.first()))
} else {
  fields(customer)
  for job in jobs {
    block(breakable: false, width: 100%)[
      #line(length: 100%, stroke: 0.5pt)
      #text(size: 11pt, strong(job.ticketNumber))
      #if job.isWarrantyRework [ \ #rework-note]
      #v(2pt)
      #if thermal [
        #fields(job-rows(job))
        #align(center, job-code(job, 16mm))
      ] else [
        #grid(columns: (1fr, auto), column-gutter: 10pt, fields(job-rows(job)), job-code(job, 18mm))
      ]
    ]
  }
  if data.estimateTotalCents != none {
    line(length: 100%, stroke: 0.5pt)
    fields(field("Total estimate", "මුළු ඇස්තමේන්තුව", strong(format-money(data.estimateTotalCents))))
  }
}

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
  #if single [#job-code(jobs.first(), if thermal { 18mm } else { 22mm }) \ ]
  #text(size: 0.85em)[Bring this receipt when you collect. \ බඩු ගන්න එනකොට මේ රිසිට්පත ගෙනෙන්න.]
]
