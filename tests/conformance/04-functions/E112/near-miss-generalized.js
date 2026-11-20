// Exported, called, or told by the body what it is: no diagnostic.
export const same = (x) => x

const wrap = (item) => [item]

export const wrapped = wrap(1)

export const words = wrap("a")

const size = (text) => text.length

export const sizes = [size("abc")]

export const first = (items) => items[0]

export const smaller = (a, b) => a < b ? a : b

const kept = (value) => value

export { kept }
