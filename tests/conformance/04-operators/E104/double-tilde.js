// expect: E104 bitwise-on-double @6:11
// `~~x` is no way to make a whole number of a double: the inner `~` is refused, and
// the finding says what to write, `Math.trunc(x)` of a number in range, or a bigint.
export function third(values) {
  const ratio = values.length / 3
  return ~~ratio
}

export const one = third([1, 2, 3])
