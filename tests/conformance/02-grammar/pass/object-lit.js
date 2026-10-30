// ObjectLit: shorthand, named, string and computed keys, a spread, and a trailing comma.
const a = 1
const k = "key"
const o = {
  a,
  b: 2,
  "c-d": 3,
  [k]: 4,
  ...a,
}
const empty = {}
f(o, empty)
