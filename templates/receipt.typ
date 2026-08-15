// =============================================================================
// PUBBLES PET PARLOR - RECEIPT TEMPLATE (TYPST)
// =============================================================================

#set document(
  title: "Pubbles Pet Parlor - Receipt",
  author: "Pubbles Pet Parlor",
)

// --- Color Palette ---
#let primary-color = rgb("#143e6a")       // Deep navy blue for headings, text, and totals
#let secondary-color = rgb("#7d9cb8")     // Muted slate blue for item numbers, strikes, borders
#let bg-color = rgb("#fcf8ec")            // Warm cream receipt paper background
#let dark-color = rgb("#103357")          // Emphasized dark blue for sale prices

// --- Page & Document Settings ---
#set page(
  width: 92mm,
  height: auto,
  margin: (x: 5.5mm, top: 8mm, bottom: 8mm),
  fill: bg-color,
)

#set text(
  font: ("Liberation Sans", "Helvetica Neue", "Arial", "DejaVu Sans"),
  size: 7.8pt,
  fill: primary-color,
  lang: "en",
)

#set par(leading: 0.45em, justify: false)

// --- Helper Functions ---
#let dotted-line() = {
  v(3pt)
  line(
    length: 100%,
    stroke: (
      paint: secondary-color,
      thickness: 0.8pt,
      dash: (array: (1.2pt, 2.4pt), phase: 0pt),
    ),
  )
  v(3pt)
}

#let item-row(
  num: "01",
  title: "",
  description: [],
  amount: 1,
  original-price: "",
  discounted-price: "",
) = {
  grid(
    columns: (1fr, auto, auto),
    column-gutter: (4pt, 12pt),
    align: (left, left + bottom, right + bottom),

    // Left Column: Item Number, Title, Description
    [
      #grid(
        columns: (16pt, 1fr),
        gutter: 2pt,
        align: (top + left, top + left),
        text(weight: "bold", size: 8.5pt, fill: secondary-color)[#num],
        [
          #text(
            font: ("Liberation Serif", "DejaVu Serif", "Georgia"),
            weight: "bold",
            size: 11pt,
            fill: primary-color,
          )[#title] \
          #v(1.5pt)
          #text(size: 7.2pt, fill: primary-color)[#description]
        ]
      )
    ],

    // Middle Column: Amount
    [
      #text(size: 7.5pt, fill: primary-color)[Amount: #amount]
      #v(1.5pt)
    ],

    // Right Column: Strikethrough Original & Discounted Price
    [
      #align(right)[
        #text(size: 7.2pt, fill: secondary-color)[#strike[#original-price]] \
        #v(1pt)
        #text(weight: "bold", size: 9pt, fill: dark-color)[#discounted-price]
      ]
    ]
  )
}

// --- LOGO HEADER ---
#align(center)[
  #v(2pt)
  #text(
    font: ("Liberation Sans", "DejaVu Sans", "Arial"),
    weight: 900,
    size: 30pt,
    fill: primary-color,
    tracking: -0.5pt,
  )[Pubbles]
  #v(8pt)
]

// --- TABLE HEADERS ---
#grid(
  columns: (1fr, auto, auto),
  column-gutter: (4pt, 22pt),
  align: (left, left, right),
  text(weight: "bold", size: 7.2pt, tracking: 0.6pt, fill: primary-color)[PURCHASED SERVICES],
  text(weight: "bold", size: 7.2pt, tracking: 0.6pt, fill: primary-color)[AMOUNT],
  text(weight: "bold", size: 7.2pt, tracking: 0.6pt, fill: primary-color)[PRICE],
)

#dotted-line()

// --- PURCHASED ITEMS ---

// Item 01
#item-row(
  num: "01",
  title: "Woof & Floof",
  description: [Full wash, dry, and coat\ brush-out for large dog],
  amount: 1,
  original-price: "$85.00",
  discounted-price: "$72.25",
)

#dotted-line()

// Item 02
#item-row(
  num: "02",
  title: "Snout & About",
  description: [Express wash and groom\ for regulars (small dog)],
  amount: 1,
  original-price: "$55.00",
  discounted-price: "$46.75",
)

#dotted-line()

// Item 03
#item-row(
  num: "03",
  title: "Paws & Jaws",
  description: [Nail trimming\ Paw pad treatment\ Teeth cleaning (regular)],
  amount: 2,
  original-price: "$36.00",
  discounted-price: "$30.60",
)

#dotted-line()
#v(4pt)

// --- TOTALS & BREAKDOWN SECTION ---
#let summary-line(label, value, is-strike: false, is-bold: false, size: 7.5pt) = {
  grid(
    columns: (1fr, auto),
    align: (left, right),
    text(weight: if is-bold { "bold" } else { "medium" }, size: size, tracking: 0.3pt, fill: primary-color)[#label],
    if is-strike {
      text(weight: if is-bold { "bold" } else { "medium" }, size: size, fill: secondary-color)[#strike[#value]]
    } else {
      text(weight: if is-bold { "bold" } else { "regular" }, size: size, fill: if is-bold { dark-color } else { primary-color })[#value]
    }
  )
  v(1pt)
}

#summary-line("ORIGINAL SUBTOTAL:", "$176.00", is-strike: true)
#summary-line("PUBBLE'S FRIEND DISCOUNT:", "-$26.40")
#summary-line("SUBTOTAL AFTER DISCOUNT:", "$149.60")
#summary-line("GST (5%):", "+$7.48")

#v(8pt)
#grid(
  columns: (1fr, auto),
  align: (left + horizon, right + horizon),
  text(weight: "bold", size: 8.5pt, tracking: 0.5pt, fill: primary-color)[TOTAL AMOUNT:],
  text(weight: "bold", size: 9.5pt, tracking: 0.3pt, fill: dark-color)[`$157.08 CAD`],
)

#v(4pt)
#dotted-line()
#v(2pt)

// --- DATE, TIME & REFERENCE CODE ---
#grid(
  columns: (1fr, 1fr, 1fr),
  align: (left, center, right),
  text(size: 7.2pt, fill: primary-color)[15/05/2025],
  text(size: 7.2pt, fill: primary-color)[12:55 PM],
  text(size: 7.2pt, fill: primary-color)[PPmay056],
)

#v(8pt)

// --- BARCODE SECTION ---
// Bars and caption are laid out as two separate stacked blocks (rather than
// overlapping `place()` calls in one box) so the caption always sits cleanly
// below the bars instead of risking overlap.
#let bar-widths = (
  1.5pt, 0.8pt, 2.2pt, 1pt, 3pt, 0.8pt, 1.5pt, 2pt, 1pt, 2.5pt,
  0.8pt, 3.2pt, 1.2pt, 2pt, 0.8pt, 2.5pt, 1.5pt, 1pt, 3pt, 1.2pt,
  2.2pt, 0.8pt, 1.5pt, 2.8pt, 1pt, 2pt, 1.2pt, 3pt, 0.8pt, 1.8pt,
  2.5pt, 1pt, 1.5pt, 2.2pt, 0.8pt, 3.2pt, 1.2pt, 2pt, 1pt, 2.5pt,
)
#let bar-spaces = (
  1.2pt, 2pt, 0.8pt, 2.5pt, 1.2pt, 1.8pt, 2.2pt, 1pt, 2.8pt, 0.8pt,
  2pt, 1.2pt, 2.5pt, 1pt, 1.8pt, 1.2pt, 2.2pt, 0.8pt, 2pt, 1.5pt,
  1pt, 2.2pt, 1.5pt, 0.8pt, 2.5pt, 1.2pt, 1.8pt, 1pt, 2.2pt, 1.5pt,
  0.8pt, 2pt, 1.2pt, 2.5pt, 1pt, 1.8pt, 1.2pt, 2.2pt, 0.8pt, 1.5pt,
)

#align(center)[
  #block(width: 82%)[
    #box(height: 28pt, width: 100%)[
      #grid(
        columns: bar-widths.len(),
        gutter: 0pt,
        align: left + horizon,
        ..range(bar-widths.len()).map(i => {
          let w = bar-widths.at(i)
          let s = bar-spaces.at(i)
          box(width: w + s, height: 28pt)[
            #align(left)[
              #rect(width: w, height: 100%, fill: primary-color, stroke: none)
            ]
          ]
        })
      )
    ]
    #v(2pt)
    #text(size: 6.2pt, weight: "bold", tracking: 0.6pt, fill: primary-color)[
      PUBBLES PET PARLOR - MAY05
    ]
  ]
]

#v(8pt)

// --- FOOTER CONTACT DETAILS ---
#align(center)[
  #text(weight: "bold", size: 7.2pt, tracking: 0.5pt, fill: primary-color)[PUBBLES PET PARLOR] \
  #v(1.5pt)
  #text(size: 6.8pt, fill: primary-color)[1435 Fernwood Road Victoria, BC] \
  #v(1.5pt)
  #text(size: 6.8pt, fill: primary-color)[+123 456 7890 #h(4pt) | #h(4pt) pubblespetparlor.com]
]
