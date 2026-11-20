// expect: E138 type-mismatch @9:29
// `half` takes a number, so `halved` does too: a string is none.
const half = (n) => n / 2

export const two = half(4)

export const halved = (x) => half(x)

export const wrong = halved("s")
