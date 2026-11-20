// expect: S005 redundant-parentheses @5:3 "(a)" -> "a"
// An argument is an assignment expression already.
import { a, f } from "./names.js"

f((a))
