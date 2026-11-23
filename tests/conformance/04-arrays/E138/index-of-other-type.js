// expect: E138 type-mismatch @5:36
// `sizes` holds numbers: a string is never found in it.
const sizes = [10, 9, 1]

export const place = sizes.indexOf("9")
