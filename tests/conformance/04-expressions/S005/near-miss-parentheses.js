// Every pair of parentheses ES2025 needs, or that the subset keeps, and a pair inside
// a template literal, which is copied as written: no diagnostic.
({ r } = s)
const a = (b ?? c) || d
const e = (b || c) ?? d
const g = (-1) ** 2
const h = (await p).x
const i = (() => 1)()
const j = typeof (b + c)
const k = -(-b)
const l = (b + c) * d
const m = b + (c + d)
const n = (b ** c) ** d
const o = (b?.c).d
const q = () => ({})
const t = `${(b)}`
