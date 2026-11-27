// expect: E124 array-index-unchecked @5:10
// Nothing shows that `at` is a place of `values`: what is read may be `undefined`,
// and it is used as a number.
export function twice(values, at) {
  return values[at] * 2
}

export const four = twice([1, 2], 1)
