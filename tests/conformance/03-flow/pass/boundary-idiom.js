// SPEC.md §6.3: what `JSON.parse` gives may be anything, and ordinary checks tell what
// it is, one after the other: no diagnostic.
export function nameOf(text) {
  const data = JSON.parse(text)
  if (typeof data !== "object" || data === null) {
    return ""
  }
  if (!Object.hasOwn(data, "name")) {
    return ""
  }
  return typeof data.name === "string" ? data.name.toUpperCase() : ""
}
