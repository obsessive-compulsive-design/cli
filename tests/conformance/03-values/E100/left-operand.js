// expect: E100 null-produced @3:34
// `null` is compared against, on the right: `value === null`.
export const isNull = (value) => null === value
