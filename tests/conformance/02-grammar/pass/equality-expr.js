// EqualityExpr: `===` and `!==`, chained with relational operands.
import { a, f } from "./names.js"

const b = a === 1

const c = a !== 2

const d = a < 2 === b

f(c, d)
