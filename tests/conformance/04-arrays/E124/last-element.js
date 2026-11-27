// expect: E124 array-index-unchecked @5:10
// An empty array has no last element: the finding says to read it with `at(-1)` and
// compare it with `undefined`.
export function last(values) {
  return values[values.length - 1] + 1
}

export const three = last([1, 2])
