// expect: E124 array-index-unchecked @8:14
// The loop's test shows that `at` is a place of `values`, and `pop` then makes the
// array shorter: the proof is cancelled.
export function drain(values) {
  let total = 0
  for (let at = 0; at < values.length; at += 1) {
    values.pop()
    total += values[at]
  }
  return total
}

export const sum = drain([1, 2, 3])
