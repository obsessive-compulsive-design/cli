// A comparator for numbers, and none for an array of strings: no diagnostic.
const sizes = [10, 9, 1]

const names = ["b", "a"]

export const ordered = [sizes.toSorted((a, b) => a - b), names.toSorted()]
