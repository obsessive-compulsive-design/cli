// expect: E137 missing-property @5:26
// `depth` is no key of the shape `box` has.
const box = { width: 1, height: 2 }

export const depth = box.depth
