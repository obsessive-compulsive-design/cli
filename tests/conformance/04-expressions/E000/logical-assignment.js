// expect: E000 syntax @4:3
// `||=` is removed: `??=` for a default, or an `if` with an explicit comparison.
let a = false
a ||= true
