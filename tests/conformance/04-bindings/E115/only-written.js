// expect: E115 unused-binding @3:5
// `count` is written twice and never read: a write is not a read.
let count = 0

count = 1

export const kept = 2
