// UnaryExpr: `!`, `-`, `+`, `~`, `typeof` and `await`, nested.
const p = Promise.resolve(1)

const a = !true

const b = -1

const c = +b

const d = ~c

const e = typeof typeof d

const g = await p

f(a, e, g)
