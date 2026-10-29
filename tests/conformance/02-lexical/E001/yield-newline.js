// expect: E001 ambiguous-line-start @5:3
// A line break between `yield` and its operand ends the `yield` first.
function* g() {
  yield
  1
}
