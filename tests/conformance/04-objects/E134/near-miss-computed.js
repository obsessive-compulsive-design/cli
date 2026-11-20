// A computed key that is another key, and one after a spread: no diagnostic.
const key = "size"

const base = { size: 0 }

export const box = { width: 1, [key]: 2 }

export const copy = { size: 1, ...base, [key]: 3 }
