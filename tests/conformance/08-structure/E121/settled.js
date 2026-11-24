// expect: E121 constant-condition @8:10
// The first test has told that `word` is a string: the second asks what is known.
export function shout(flag) {
  const word = flag ? "a" : 1
  if (typeof word !== "string") {
    return ""
  }
  return typeof word === "string" ? word.toUpperCase() : ""
}
