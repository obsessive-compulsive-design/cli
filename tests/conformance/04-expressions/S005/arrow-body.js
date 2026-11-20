// expect: S005 redundant-parentheses @5:17 "(a + b)" -> "a + b"
// An expression body is an assignment expression already.
import { a, b } from "./names.js"

const f = () => (a + b)

export { f }
