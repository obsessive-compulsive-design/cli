// expect: S005 redundant-parentheses @5:11 "(a + b)" -> "a + b"
// The left operand of `+` binds as tightly without the pair.
import { a, b } from "./names.js"

const c = (a + b) + 1

export { c }
