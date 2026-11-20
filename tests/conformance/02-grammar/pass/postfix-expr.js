// PostfixExpr: members, optional members, indexes, optional indexes, calls, optional calls.
import { f, o } from "./names.js"

const a = o.a.b[0]

const b = o?.a?.b?.[0]

const c = o.f()

const d = o.f?.()

f(a, b, c, d)
