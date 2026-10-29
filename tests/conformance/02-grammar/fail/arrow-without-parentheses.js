// expect: E000 syntax @3:11 "a" -> "(a)"
// ArrowParameters without parentheses are removed.
const f = a => a
