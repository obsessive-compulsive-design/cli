// expect: E139 not-callable @5:25
// `make` is a function, not a class.
const make = () => ({ size: 1 })

export const made = new make()
