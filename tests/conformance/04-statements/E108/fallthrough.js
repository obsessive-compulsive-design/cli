// expect: E108 fallthrough @6:5
// The block of `case 1` ends without leaving the `switch`: it runs on into `case 2`.
export function describe(level) {
  let text = "none"
  switch (level) {
    case 1: {
      text = "low"
    }
    case 2: {
      text = "high"
      break
    }
    default: {
      text = "unknown"
    }
  }
  return text
}
