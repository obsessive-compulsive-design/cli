// Every permitted expression form: no diagnostic.
let i = 0
let a = 1
let b = 2
const c = a < b === b > a
const d = (a ?? b) || c
const e = a ?? (b || c)
const f = (-2) ** 2 + 2 ** -2
const g = typeof typeof a
const h = a ? b : c ? d : e
const k = new Map().size
const m = a & b | a ^ ~b << 1 >> 1 >>> 1
i += 1
i -= 1
i *= 2
i ??= 3
i **= 2
const o = { a, b: 1, "c": 2, [a]: 3 }
const r = /a+/v.test("aa")
const t = `${a} and ${`${b}`}`
const n = await Promise.resolve(1)
const p = o?.a.b
const q = o?.[0]?.()
const s = String.raw`${a}\n`
const u = new Map().size + new Map().size
const w = await import("./w.js")
const y = o.delete(1)
function* gen() {
  yield
  yield 1
  yield* gen()
  const v = yield a
}
async function run() {
  await run()
}
[a, b] = [b, a]
