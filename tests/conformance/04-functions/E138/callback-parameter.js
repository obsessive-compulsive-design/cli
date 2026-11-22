// expect: E138 type-mismatch @5:38
// `greet` calls what it is given with a string, so `n` is one: it is not halved.
const greet = (f) => f("a")

export const halved = greet((n) => n / 2)
