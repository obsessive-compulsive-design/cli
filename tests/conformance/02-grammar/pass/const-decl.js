// ConstDecl: a name, an object pattern and an array pattern, each initialised.
const o = { a: 1, b: 2 }
const xs = [1, 2]
const a = 1
const { b, c = 3 } = o
const [d, ...e] = xs
f(a, b, c, d, e)
