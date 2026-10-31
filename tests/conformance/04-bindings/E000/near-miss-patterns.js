// Every pattern form: no diagnostic.
const a = 1

let b = a + 1

const { c, d: e, f = 1, g: [h, i = h], ...j } = k

const [l, m = l, { n }, ...o] = p

const {} = q
