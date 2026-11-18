// expect: E106 unused-let @3:1 "let" -> "const"
// `count` is given a value once and never another.
let count = 1

export { count }
