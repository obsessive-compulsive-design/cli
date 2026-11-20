// A shape, `undefined` and a union of the two spread: no diagnostic.
const base = { size: 0 }

let maybe = undefined

maybe = { depth: 1 }

export const box = { ...base, ...undefined, ...maybe, width: 1 }
