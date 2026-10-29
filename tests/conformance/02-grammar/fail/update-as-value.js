// expect: E000 syntax @4:11
// `++`/`--` as a value is removed: `i += 1` on its own line.
let i = 0
const j = i++
