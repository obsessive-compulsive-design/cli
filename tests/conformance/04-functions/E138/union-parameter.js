// expect: E138 type-mismatch @4:23
// `half` is called with a string and with a number: `/` cannot take both.
// (The finding names each call.)
const half = (x) => x / 2

export const halves = [half("a"), half(1)]
