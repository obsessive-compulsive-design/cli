// expect: S005 redundant-parentheses @3:17 "(a + b)" -> "a + b"
// An expression body is an assignment expression already.
const f = () => (a + b)
