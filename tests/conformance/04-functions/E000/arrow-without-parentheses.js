// expect: E000 syntax @3:11 "a" -> "(a)"
// Arrow parameters are always parenthesised (SPEC.md §2 `ArrowFn`).
const f = a => a
