// Every permitted literal form: no diagnostic.
const k = "key"
const o = { a: 1, b: 2, "c-d": 3, [k]: 4, ...o }
const p = { a: 1, ...o, a: 2 }
const q = { async: 1, get: 2, set: 3 }
const r = { ["__proto__"]: p }
const s = { __proto__ }
const t = {}
const u = []
const v = [1, ...u, ...u]
const w = { a: 1, b: { a: 2 } }
