// expect: E117 unreachable-code @12:5
// Every value of `level` is named by a `case`: nothing is left for the `default`.
export function weight(flag) {
  const level = flag ? "low" : "high"
  switch (level) {
    case "low": {
      return 1
    }
    case "high": {
      return 2
    }
    default: {
      return 0
    }
  }
}
