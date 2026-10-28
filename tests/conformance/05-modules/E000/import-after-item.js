// expect: E000 syntax @4:1
// Imports precede every other module item (SPEC.md §2 `Module`).
const a = 1
import { b } from "./b.js"
