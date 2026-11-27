// expect: E104 bitwise-on-double @6:21
// A length is an `int32`, and twice a length may be more than one holds: the product
// is a number, which `&` does not take.
export function isEven(values) {
  const count = values.length
  return (count * 2 & 1) === 0
}

export const even = isEven([1, 2])
