// ObjectLit: shorthand, named, string and computed keys, a spread, and a trailing comma.
import { f } from "./names.js"

const a = 1

const k = "key"

const o = {
  a,
  b: 2,
  "c-d": 3,
  [k]: 4,
  ...f,
}

const empty = {}

f(o, empty)
