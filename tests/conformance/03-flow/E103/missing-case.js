// expect: E103 non-exhaustive @6:11
// `level` is one of three, and no `case` names `"high"`: the finding lists it.
export function weight(flag, other) {
  const level = flag ? "low" : other ? "mid" : "high"
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
  }
  return total
}
