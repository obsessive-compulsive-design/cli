// CondExpr: a conditional, nested to the right and in each branch.
const a = true
const b = a ? 1 : 2
const c = a ? b === 1 ? 3 : 4 : 5
const d = a ? (b ? 6 : 7) : 8
f(c, d)
