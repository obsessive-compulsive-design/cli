// expect: E000 syntax @4:3
// LogicalAssignment with `||=`/`&&=` is removed: `??=` or an `if`.
let a = false
a ||= true
