// expect: E134 duplicate-key @3:19
// The later value wins, so the first is dead (SPEC.md §4.6).
const o = { a: 1, a: 2 }
