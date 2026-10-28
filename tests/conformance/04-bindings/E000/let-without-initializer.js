// expect: E000 syntax @3:1 "let x" -> "let x = undefined"
// The implicit `undefined` of a bare `let` is written out (SPEC.md §12).
let x
