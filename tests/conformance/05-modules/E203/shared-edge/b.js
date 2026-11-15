// expect: E203 import-cycle @3:19
// `a.js -> b.js -> a.js`.
import { a } from "./a.js"
import { e } from "./entry.js"

export const b = [a, e]
