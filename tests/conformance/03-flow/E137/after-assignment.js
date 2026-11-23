// expect: E137 missing-property @9:15
// The assignment undoes what the test told: `word` may be `undefined` again.
export function shout(flag) {
  let word = flag ? "a" : undefined
  if (word === undefined) {
    return ""
  }
  word = flag ? "b" : undefined
  return word.toUpperCase()
}
