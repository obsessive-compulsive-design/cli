// expect: E137 missing-property @7:25
// `key` is a `let`: which key it names is not known, so the read is not either.
const box = { width: 1, height: 2 }

let key = "width"

export const side = box[key]

key = "height"
