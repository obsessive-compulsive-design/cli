// expect: S005 redundant-parentheses @3:11 "(a + b)" -> "a + b"
// The left operand of `+` binds as tightly without the pair.
const c = (a + b) + 1
