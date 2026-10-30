// expect: S005 redundant-parentheses @3:11 "((a))" -> "(a)"
// expect: S005 redundant-parentheses @3:12 "(a)" -> "a"
const b = ((a))
