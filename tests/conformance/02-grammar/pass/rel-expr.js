// RelExpr: `<`, `>`, `<=`, `>=` and `instanceof`.
class A {}

const a = new A()

const n = 1

const r = [n < 2, n > 0, n <= 1, n >= 1, a instanceof A]

f(r)
