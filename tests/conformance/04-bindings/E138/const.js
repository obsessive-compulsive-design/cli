// expect: E138 type-mismatch @5:1
// A `const` is given its value once.
const limit = 1

limit = 2

export { limit }
