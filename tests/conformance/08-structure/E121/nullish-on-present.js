// expect: E121 constant-condition @5:10
// `count` is a number, never `undefined` or `null`: the right of `??` is never used.
export function orZero(flag) {
  const count = flag ? 1 : 2
  return count ?? 0
}
