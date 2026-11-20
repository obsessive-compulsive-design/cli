// expect: E100 null-produced @4:22
// expect: E100 null-produced @6:25
// In an array and in an object it is made all the same.
export const list = [null]

export const box = { a: null }
