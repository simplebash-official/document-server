// Expected JSON input (POST /api/render/{invoice-template-key} body):
// {
//   "invoiceNumber": "string",
//   "customerName": "string",
//   "items": [{ "description": "string", "quantity": number, "unitPrice": number }],
//   "total": number
// }
#let data = sys.inputs

#set page(width: 80mm, height: auto, margin: 8mm)
#set text(font: "Noto Sans", size: 10pt)

= Invoice #data.invoiceNumber
Customer: #data.customerName

#table(
  columns: 3,
  [*Item*], [*Qty*], [*Price*],
  ..data.items.map(item => (item.description, str(item.quantity), str(item.unitPrice))).flatten()
)

*Total: #data.total*
