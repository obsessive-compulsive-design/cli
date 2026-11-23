// Every block but the last ends in `return`, `throw`, `break` or `continue`, an
// `if`/`else` whose branches both return among them; stacked labels share one block:
// no diagnostic.
export function total(kinds) {
  let sum = 0
  for (const kind of kinds) {
    switch (kind) {
      case "skip": {
        continue
      }
      case "one":
      case "single": {
        sum += 1
        break
      }
      case "sign": {
        if (sum < 0) {
          return -1
        } else {
          return 1
        }
      }
      case "bad": {
        throw new Error("a bad kind")
      }
      default: {
        sum += 2
      }
    }
  }
  return sum
}
