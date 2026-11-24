// expect: E103 non-exhaustive @6:3
// Each branch leaves and nothing follows the chain: it is a `switch` written out, and
// it has no branch for `"high"`.
export function report(flag, other, log) {
  const level = flag ? "low" : other ? "mid" : "high"
  if (level === "low") {
    log(1)
    return
  } else if (level === "mid") {
    log(2)
    return
  }
}
