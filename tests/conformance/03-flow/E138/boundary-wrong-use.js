// expect: E138 type-mismatch @11:52
// The checks tell that `data.name` is a string: it is not added to a number.
export function nameOf(text) {
  const data = JSON.parse(text)
  if (typeof data !== "object" || data === null) {
    return 0
  }
  if (!Object.hasOwn(data, "name")) {
    return 0
  }
  return typeof data.name === "string" ? data.name + 1 : 0
}
