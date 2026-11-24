// expect: E105 no-overlap @11:10
// `level` is `"low"` or `"high"`, never `"mid"`: the `case` is never taken.
export function weight(flag) {
  const level = flag ? "low" : "high"
  let total = 0
  switch (level) {
    case "low": {
      total += 1
      break
    }
    case "mid": {
      total += 2
      break
    }
    default: {
      total += 3
    }
  }
  return total
}
