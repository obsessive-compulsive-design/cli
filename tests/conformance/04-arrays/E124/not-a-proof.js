// expect: E124 array-index-unchecked @8:14
// expect: E124 array-index-unchecked @11:14
// `<=` lets the length itself through, and the length of another array says nothing
// of this one.
export function sum(values, others) {
  let total = 0
  for (let at = 0; at <= values.length; at += 1) {
    total += values[at]
  }
  for (let at = 0; at < others.length; at += 1) {
    total += values[at]
  }
  return total
}

export const six = sum([1, 2], [3])
