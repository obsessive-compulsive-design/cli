// expect: E115 unused-binding @3:24
// A parameter before one that is read is unused all the same: name it `_first`.
export const second = (first, other) => other
