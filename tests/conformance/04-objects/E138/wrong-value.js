// expect: E138 type-mismatch @5:13
// `width` holds a number.
export const box = { width: 1 }

box.width = "wide"
