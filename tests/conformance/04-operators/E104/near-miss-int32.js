// What is known to be an `int32` goes where one is taken: a length, a place found, the
// variable of a loop that counts up to a length, a whole number of a length divided,
// and the results of the bitwise operators and of `%` on those: no diagnostic.
export function mix(values) {
  const half = Math.floor(values.length / 2)
  let bits = values.indexOf(1) & half
  for (let at = 0; at < values.length; at += 1) {
    bits = bits ^ at | at % 2
  }
  return bits << 1 >> 1
}

export const mixed = mix([1, 2, 3])
