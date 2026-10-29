// expect: E001 ambiguous-line-start @5:3
// A line break between `throw` and its operand is a syntax error in ES2025.
function f() {
  throw
  new Error("x")
}
