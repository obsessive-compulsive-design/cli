// expect: E001 ambiguous-line-start @4:1
// `x` then `/re/v.test(y)` reads as `x / re / v.test(y)`.
const ok = x
/re/v.test(y)
