// expect: E115 unused-binding @4:7
// expect: E112 unconstrained-parameter @4:15
// Nothing calls `same` and it is not exported: nothing says what `x` is.
const same = (x) => x
