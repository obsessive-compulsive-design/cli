// expect: E102 ambiguous-truthiness @6:7
// A length is a number, and `0` is one of them: what is asked is whether it is more.
// The finding says what to write: `items.length > 0`.
export function first(flag) {
  const items = flag ? [1] : [2, 3]
  if (items.length) {
    return items.at(0)
  }
  return undefined
}
