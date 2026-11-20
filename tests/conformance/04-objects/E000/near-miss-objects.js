// Every permitted literal form: no diagnostic.
const k = "key"

const base = { z: 0 }

const o = { a: 1, b: 2, "c-d": 3, [k]: 4, ...base }

const p = { a: 1, ...o, a: 2 }

const q = { async: 1, get: 2, set: 3 }

const r = { ["__proto__"]: p }

const t = {}

const u = [0]

const v = [1, ...u, ...u]

const w = { a: 1, b: { a: 2 } }

export { q, r, t, v, w }
