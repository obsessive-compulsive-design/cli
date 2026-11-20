// expect: E136 arity-mismatch @5:20
// `add` has two parameters and neither has a default.
const add = (a, b) => a + b

export const sum = add(1)
