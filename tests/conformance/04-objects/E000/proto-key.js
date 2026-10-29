// expect: E000 syntax @3:13
// `__proto__: …` is prototype manipulation, which is removed (SPEC.md §12).
const o = { __proto__: p }
