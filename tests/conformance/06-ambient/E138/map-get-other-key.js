// expect: E138 type-mismatch @5:29
// `ages` is keyed by strings: a number is never one of its keys.
const ages = new Map([["ada", 36]])

export const age = ages.get(36)
