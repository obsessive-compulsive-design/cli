// expect: E203 import-cycle @3:19
// `a.js -> b.js -> entry.js -> a.js`: it shares `a.js -> b.js` with the cycle in `b.js`.
import { a } from "./a.js"

export const e = a
