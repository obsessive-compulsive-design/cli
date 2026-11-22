// expect: E136 arity-mismatch @5:36
// `each` passes its callback one argument: nothing is ever passed for `index`.
const each = (f) => f("a")

export const lengths = each((item, index) => item.length + index)
