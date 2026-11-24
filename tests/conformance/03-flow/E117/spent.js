// expect: E117 unreachable-code @14:10
// The `switch` names every value of `level` and each `case` leaves: past it nothing
// is left of `level`, and a use of it is never reached.
export function weight(flag) {
  const level = flag ? "low" : "high"
  switch (level) {
    case "low": {
      return "1"
    }
    case "high": {
      return "2"
    }
  }
  return level
}
