// EqualityExpr: `===` and `!==`, chained with relational operands.
import { f } from "./names.js"

const a = 1

const b = a === 1

const c = a !== 2

const d = a < 2 === b

f(c, d)
