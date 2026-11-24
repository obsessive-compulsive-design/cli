// expect: E138 type-mismatch @13:36
// `isNamed` tells that `data.name` is a string: it is not added to a number.
const isObject = (data) => typeof data === "object" && data !== null

const hasName = (data) => isObject(data) && Object.hasOwn(data, "name")

function isNamed(data) {
  return hasName(data) && typeof data.name === "string"
}

export function nameOf(text) {
  const data = JSON.parse(text)
  return isNamed(data) ? data.name + 1 : 0
}
