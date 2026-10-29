// expect: E001 ambiguous-line-start @5:3
// A line break between `return` and its value ends the `return` first.
function f() {
  return
  1
}
