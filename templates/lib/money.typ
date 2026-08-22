// Shared money formatting for document templates. Mirrors the frontend's
// `formatMoney()` (src/shared/lib/money.ts): amounts arrive as integer
// cents, are formatted with two decimal places and a thousands separator.

/// Formats integer cents into "Rs. 1,234.50" (or "-Rs. 1,234.50" for a negative
/// amount). `symbol` defaults to "Rs." to match `CURRENCY.symbol`.
#let format-money(cents, symbol: "Rs.") = {
  let negative = cents < 0
  let abs-cents = if negative { -cents } else { cents }
  let whole = int(calc.floor(abs-cents / 100))
  let remainder = int(abs-cents - whole * 100)

  let digits = str(whole)
  let groups = ()
  let i = digits.len()
  while i > 3 {
    groups.push(digits.slice(i - 3, i))
    i -= 3
  }
  groups.push(digits.slice(0, i))
  groups = groups.rev()

  let rem-str = if remainder < 10 { "0" + str(remainder) } else { str(remainder) }

  symbol + " " + (if negative { "-" } else { "" }) + groups.join(",") + "." + rem-str
}
