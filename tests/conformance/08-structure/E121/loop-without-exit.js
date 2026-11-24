// expect: E121 constant-condition @4:10
// The body of `while (true)` has no `break`, `return` or `throw`: the loop never ends.
export function spin(tick) {
  while (true) {
    tick()
  }
}
