// Expected JSON input (POST /api/render/{sticker-template-key} body):
// {
//   "title": "string",      // product/repair name printed above the codes
//   "reference": "string"   // encoded as both the Code128 barcode and the QR code payload
// }
//
// Sized for a 50mm x 30mm thermal label — a common print-label size for
// product/repair-ticket stickers.
#import "../lib/codes.typ": barcode, qrcode

#let data = sys.inputs

#set page(width: 50mm, height: 30mm, margin: 2mm)
#set text(font: "Noto Sans", size: 6pt)

#grid(
  columns: (1fr, auto),
  align: horizon,
  column-gutter: 2mm,
  [
    #data.title
    #v(2pt)
    #barcode(data.reference, height: 8mm, show-text: false)
    #v(1pt)
    #data.reference
  ],
  qrcode(data.reference, width: 14mm),
)
