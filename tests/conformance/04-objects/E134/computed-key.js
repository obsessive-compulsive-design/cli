// expect: E134 duplicate-key @5:32
// `key` is the literal "size": the same key as the one spelled outright.
const key = "size"

export const box = { size: 1, [key]: 2 }
