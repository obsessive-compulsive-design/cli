// expect: E115 unused-binding @4:10
// expect: E112 unconstrained-parameter @4:15
// A function nothing calls, whose body only stores its parameter.
function wrap(item) {
  return [item]
}
