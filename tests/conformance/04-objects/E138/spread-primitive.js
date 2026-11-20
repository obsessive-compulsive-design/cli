// expect: E138 type-mismatch @5:31
// Nor does a string.
const label = "size"

export const box = { size: 1, ...label }
