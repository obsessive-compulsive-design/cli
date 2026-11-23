// expect: E117 unreachable-code @6:3
// Nothing runs after the `return`: the two statements are one stretch, reported once.
export function double(value) {
  let result = value * 2
  return result
  result += 1
  return result
}
