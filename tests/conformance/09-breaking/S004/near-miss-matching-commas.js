// A single-line list without a comma, a kept multi-line object with one, and a
// parameter list ending in a rest parameter: no diagnostic.
const a = [1, 2]
const o = {
  a: 1,
  b: 2,
}
function f(aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa, ...rest) {}
