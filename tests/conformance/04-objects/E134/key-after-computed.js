// expect: E134 duplicate-key @5:32
// A key spelled outright repeats a computed one just the same.
const key = "size"

export const box = { [key]: 1, size: 2 }
