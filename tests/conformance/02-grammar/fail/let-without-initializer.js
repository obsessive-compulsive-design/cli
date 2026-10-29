// expect: E000 syntax @3:1 "let x" -> "let x = undefined"
// LetDecl without an initializer is removed: `undefined` written out.
let x
