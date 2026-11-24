// expect: E137 missing-property @11:30
// The body of `isWord` is two statements: it is a function that gives a boolean, and
// tells nothing of what it is passed.
function isWord(value) {
  const kind = typeof value
  return kind === "string"
}

export function shout(flag) {
  const word = flag ? "a" : 1
  return isWord(word) ? word.toUpperCase() : ""
}
