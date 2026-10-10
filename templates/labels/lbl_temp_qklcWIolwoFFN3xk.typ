// Product Sticker Label — plain example. Input: { title, reference }
// (see the sibling .schema.json). 50mm x 30mm thermal label.
#import "../lib/codes.typ": barcode

#let data = sys.inputs

#set page(width: 50mm, height: 30mm, margin: 2mm)
#set text(font: "Noto Sans", size: 7pt)

#align(center)[
  #strong(data.title)
  #v(2pt)
  #barcode(data.reference, height: 12mm, show-text: false)
  #data.reference
]
