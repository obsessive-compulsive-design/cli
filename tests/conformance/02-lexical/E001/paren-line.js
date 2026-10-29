// expect: E001 ambiguous-line-start @4:1
// A line starting with `(` continues the expression before it as a call.
const total = sum
(1 + 2).toFixed(2)
