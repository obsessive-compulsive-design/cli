// expect: E136 arity-mismatch @5:20
// A third argument has no parameter to go to.
const add = (a, b) => a + b

export const sum = add(1, 2, 3)
