// expect: E137 missing-property @7:28
// `shape` may be `undefined`, which has no `width`.
let shape = undefined

shape = { width: 1 }

export const width = shape.width
