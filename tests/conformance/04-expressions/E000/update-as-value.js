// expect: E000 syntax @4:11
// `i++` as a value: the rewrite to `i += 1` would change the result.
let i = 0
const j = i++
