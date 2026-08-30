// =============================================================================
// charts.typ — tiny native-Typst chart primitives for report templates.
// =============================================================================
// Hand-rolled (no `@preview` package) on purpose: the render engine has no
// network package resolver, cetz/cetz-plot are LGPL and slow to compile, and
// a business report only needs bars + a line. Everything here is built from
// `rect`, `line`, `place`, `grid` and `layout`.
//
// Colours are passed in by the caller so a template controls its own palette.
// Values are plain numbers (the caller converts cents -> rupees first).

#let _axis = rgb("#E4E6EB")
#let _muted = rgb("#5B6270")

/// Vertical bar chart. `values` and `labels` are equal-length arrays.
/// `colors` (optional) cycles per bar; otherwise every bar is `color`.
#let column-chart(
  values,
  labels,
  color: rgb("#3D4EAC"),
  colors: none,
  height: 84pt,
  fmt: v => str(calc.round(v)),
) = {
  if values.len() == 0 { return [] }
  let maxv = calc.max(..values, 1)
  grid(
    columns: values.len() * (1fr,),
    column-gutter: 5pt,
    row-gutter: 3pt,
    ..values.enumerate().map(((i, v)) => {
      let c = if colors == none { color } else { colors.at(calc.rem(i, colors.len())) }
      align(bottom + center, stack(
        dir: ttb,
        spacing: 2pt,
        text(size: 6pt, fill: _muted)[#fmt(v)],
        rect(
          width: 100%,
          height: calc.max(v / maxv * height, 0.5pt),
          fill: c,
          radius: (top-left: 2pt, top-right: 2pt),
        ),
      ))
    }),
    ..labels.map(l => align(center, text(size: 6pt, fill: _muted)[#l])),
  )
}

/// Stacked vertical bar chart. `series` is an array of
/// `(name: str, color: color, values: array)`, all `values` the same length as
/// `labels`.
#let stacked-column-chart(labels, series, height: 84pt, fmt: v => str(calc.round(v))) = {
  if labels.len() == 0 or series.len() == 0 { return [] }
  let totals = labels
    .enumerate()
    .map(((i, _)) => series.fold(0, (acc, s) => acc + s.values.at(i, default: 0)))
  let maxv = calc.max(..totals, 1)
  grid(
    columns: labels.len() * (1fr,),
    column-gutter: 5pt,
    row-gutter: 3pt,
    ..labels
      .enumerate()
      .map(((i, _)) => align(bottom + center, stack(
        dir: btt,
        ..series.map(s => rect(
          width: 100%,
          height: s.values.at(i, default: 0) / maxv * height,
          fill: s.color,
        )),
      ))),
    ..labels.map(l => align(center, text(size: 6pt, fill: _muted)[#l])),
  )
}

/// Horizontal bar chart — best for ranked lists (categories, customers).
/// `rows` is an array of `(label, value)` (2-tuples) or `(label, value, sub)`.
#let hbar-chart(rows, color: rgb("#3D4EAC"), fmt: v => str(calc.round(v))) = {
  if rows.len() == 0 { return [] }
  let maxv = calc.max(..rows.map(r => r.at(1)), 1)
  grid(
    columns: (7em, 1fr, auto),
    column-gutter: 8pt,
    row-gutter: 5pt,
    ..rows
      .map(r => (
        text(size: 7.5pt)[#r.at(0)],
        box(width: 100%, height: 9pt, {
          place(horizon, line(
            start: (0pt, 0pt),
            end: (100%, 0pt),
            stroke: 0.5pt + _axis,
          ))
          rect(
            width: r.at(1) / maxv * 100%,
            height: 9pt,
            fill: color,
            radius: 1.5pt,
          )
        }),
        text(size: 7.5pt, weight: 600)[#fmt(r.at(1))],
      ))
      .flatten(),
  )
}

/// Multi-series line chart drawn with straight segments. `series` is an array
/// of `(name: str, color: color, values: array)`, each `values` the same
/// length as `labels`.
#let line-chart(labels, series, height: 96pt, fmt: v => str(calc.round(v))) = {
  if labels.len() == 0 or series.len() == 0 { return [] }
  let all = series.map(s => s.values).flatten()
  let maxv = calc.max(..all, 1)
  let n = labels.len()
  layout(size => {
    let w = size.width
    let px(i) = if n <= 1 { w / 2 } else { i / (n - 1) * w }
    let py(v) = height - v / maxv * height
    box(width: 100%, height: height, {
      // horizontal gridlines
      for g in range(0, 5) {
        place(line(
          start: (0pt, g / 4 * height),
          end: (w, g / 4 * height),
          stroke: 0.4pt + _axis,
        ))
      }
      for s in series {
        let pts = s.values.enumerate().map(((i, v)) => (px(i), py(v)))
        for i in range(0, pts.len() - 1) {
          place(line(start: pts.at(i), end: pts.at(i + 1), stroke: 1.4pt + s.color))
        }
        for p in pts {
          place(dx: p.at(0) - 1.5pt, dy: p.at(1) - 1.5pt, circle(
            radius: 1.5pt,
            fill: s.color,
            stroke: none,
          ))
        }
      }
    })
    v(2pt)
    grid(
      columns: n * (1fr,),
      ..labels.map(l => align(center, text(size: 6pt, fill: _muted)[#l])),
    )
  })
}

/// A single proportional bar plus a legend — a lightweight "pie" replacement.
/// `slices` is an array of `(label, value, color)`.
#let share-bar(slices, fmt: v => str(calc.round(v))) = {
  let total = slices.fold(0, (acc, s) => acc + s.at(1))
  if total <= 0 { return [] }
  stack(
    dir: ttb,
    spacing: 6pt,
    box(width: 100%, height: 14pt, radius: 3pt, clip: true, stack(
      dir: ltr,
      ..slices.map(s => rect(width: s.at(1) / total * 100%, height: 14pt, fill: s.at(2))),
    )),
    grid(
      columns: (1fr, 1fr, 1fr, 1fr),
      column-gutter: 6pt,
      row-gutter: 3pt,
      ..slices.map(s => box(stack(
        dir: ltr,
        spacing: 4pt,
        box(width: 7pt, height: 7pt, radius: 1pt, fill: s.at(2)),
        text(size: 6.5pt, fill: _muted)[#s.at(0) · #fmt(s.at(1))],
      ))),
    ),
  )
}
