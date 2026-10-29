// expect: E001 ambiguous-line-start @6:1
// expect: S012 increment-statement @6:1 "++i" -> "i += 1"
// A `++` on its own line reads as the end of the line before, but starts a statement.
let i = 0
i
++i
