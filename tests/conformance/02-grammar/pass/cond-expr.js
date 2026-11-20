// CondExpr: a conditional, nested to the right and in each branch.
import { f } from "./names.js"

const a = true

const b = a ? 1 : 2

const c = a ? b === 1 ? 3 : 4 : 5

const d = (a ? a : b === 2) ? 8 : 9

f(c, d)
