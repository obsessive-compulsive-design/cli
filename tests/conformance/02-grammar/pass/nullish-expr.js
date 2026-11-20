// NullishExpr: `??` alone, and parenthesised beside `||` and `&&`.
import { a, f } from "./names.js"

const b = a ?? 1

const c = (a ?? b) || false

const d = a ?? (b && c)

f(d)
