// PrimaryExpr: each literal, a name, `this`, an array, an object, an arrow, a template,
// `new` on a name and a namespaced name, a dynamic import, and parentheses.
import * as ns from "./a.js"
class A {
  m() {
    return this
  }
}
const a = [1, "s", true, null, 2n, /re/v]
const b = a
const c = { k: 1 }
const d = (x) => x
const e = `${b}`
const g = new A()
const h = new ns.B()
const i = await import("./i.js")
const j = (1)
f(c, d, e, g, h, i, j)
