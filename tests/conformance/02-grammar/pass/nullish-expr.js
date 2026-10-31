// NullishExpr: `??` alone, and parenthesised beside `||` and `&&`.
const a = undefined

const b = a ?? 1

const c = (a ?? b) || false

const d = a ?? (b && c)

f(d)
