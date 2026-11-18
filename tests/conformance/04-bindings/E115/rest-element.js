// expect: E115 unused-binding @3:33
// A rest element is a binding like the names before it.
export const head = ([first, ...others]) => first
