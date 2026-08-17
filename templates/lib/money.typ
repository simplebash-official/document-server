// Shared money formatting for document templates. Mirrors the frontend's
// `formatMoney()` (src/shared/lib/money.ts): amounts arrive as integer
// cents, are divided by 100, and rendered with zero decimal places and a
// thousands separator (the shop's currency — LKR — is configured with
// `CURRENCY.decimals = 0` app-wide, so this must match, not just look close).

/// Formats integer cents into "Rs. 1,234" (or "-Rs. 1,234" for a negative
/// amount). `symbol` defaults to "Rs." to match `CURRENCY.symbol`.
#let format-money(cents, symbol: "Rs.") = {
  let whole = int(calc.round(float(cents) / 100))
  let negative = whole < 0
  if negative { whole = -whole }

  let digits = str(whole)
  let groups = ()
  let i = digits.len()
  while i > 3 {
    groups.push(digits.slice(i - 3, i))
    i -= 3
  }
  groups.push(digits.slice(0, i))
  groups = groups.rev()

  symbol + " " + (if negative { "-" } else { "" }) + groups.join(",")
}
