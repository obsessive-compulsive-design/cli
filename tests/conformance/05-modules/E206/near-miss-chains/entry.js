// A rename, a chain of depth 5, an import exported again, an exported `let`, a
// namespace read by the names it has: no diagnostic.
import { a, deep, r as s } from "./lib.js"
import * as ns from "./lib.js"

export let count = ns.a + ns["r"]

count += s + deep

export { a as b }
