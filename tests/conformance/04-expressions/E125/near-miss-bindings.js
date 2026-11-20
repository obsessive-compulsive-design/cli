// A `let` that only ever holds numbers, a destructured string and a default that
// stands in for `undefined`: no diagnostic.
let count = 0

count += 1

const { name, missing = "none" } = { name: "a", missing: undefined }

export const label = `${count} ${name} ${missing}`
