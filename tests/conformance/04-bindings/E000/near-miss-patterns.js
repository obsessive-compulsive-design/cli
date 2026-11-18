// Every pattern form: no diagnostic.
const a = 1

const b = a + 1

const { c, d: e, f = 1, g: [h, i = h], ...j } = k

const [l, m = l, { n }, ...o] = p

const {} = q

export { b, c, e, f, i, j, m, n, o }
