// expect: E000 syntax @5:23
// `key` is a `let`: which key it names is not known when the literal is made.
let key = "size"

export const box = { [key]: 1 }

key = "width"
