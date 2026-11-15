// `shared.js` is reached by two paths, and `b.js` imports its sibling: no path returns.
import { a } from "./a.js"
import { b } from "./b.js"

export const e = [a, b]
