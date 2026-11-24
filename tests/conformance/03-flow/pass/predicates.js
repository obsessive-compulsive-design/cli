// A function whose body is one `return` of a test of its parameter is that test where
// it is called: one built from others, one over what may be anything, and one passed
// to `filter`: no diagnostic.
const isString = (value) => typeof value === "string"

const isObject = (data) => typeof data === "object" && data !== null

const hasName = (data) => isObject(data) && Object.hasOwn(data, "name")

function isNamed(data) {
  return hasName(data) && typeof data.name === "string"
}

export function nameOf(text) {
  const data = JSON.parse(text)
  return isNamed(data) ? data.name.toUpperCase() : ""
}

export function shout(flag) {
  const word = flag ? "a" : 1
  if (!isString(word)) {
    return ""
  }
  return word.toUpperCase()
}

export function words(flag) {
  const mixed = [flag ? "a" : 1, "b"]
  return mixed.filter(isString).map((word) => word.toUpperCase())
}
