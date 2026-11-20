// Expr: an assignment expression as a statement, and as an initializer.
import { f } from "./names.js"

let a = 1

a = a + 1

const b = a === 2 ? a : 0

f(a, b)
