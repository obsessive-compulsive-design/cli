// expect: S005 redundant-parentheses @5:11 "((a))" -> "(a)"
// expect: S005 redundant-parentheses @5:12 "(a)" -> "a"
import { a } from "./names.js"

const b = ((a))

export { b }
