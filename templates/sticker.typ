// Expected JSON input (POST /api/render/{sticker-template-key} body):
// {
//   "title": "string",      // product/repair name printed above the codes
//   "reference": "string"   // encoded as both the Code128 barcode and the QR code payload
// }
//
// Sized for a 50mm x 30mm thermal label — a common print-label size for
// product/repair-ticket stickers, not just an on-screen preview size.
#import "lib/tiaoma/lib.typ": code128
#import "lib/zebra/lib.typ": qrcode

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
    #code128(data.reference, height: 8mm)
    #v(1pt)
    #data.reference
  ],
  qrcode(data.reference, width: 14mm),
)
