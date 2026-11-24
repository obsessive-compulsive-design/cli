// expect: E102 ambiguous-truthiness @6:10
// `name` is a string or `undefined`, and a string may be empty: the condition does not
// tell the two apart. The finding says what to write: `name !== undefined`.
export function greet(flag) {
  const name = flag ? `${flag}` : undefined
  return name ? name.toUpperCase() : "nobody"
}
