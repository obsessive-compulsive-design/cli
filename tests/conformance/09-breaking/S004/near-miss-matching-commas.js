// expect: S006 line-too-long @2:1
// expect: S006 line-too-long @13:12 "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa, ...rest" -> "\n  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,\n  ...rest\n"
// A single-line list without a comma, a kept multi-line object with one, and a
// parameter list ending in a rest parameter, which the layout breaks without a comma
// after the rest: no S004.
const a = [1, 2]

const o = {
  a: 1,
  b: 2,
}

function f(aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa, ...rest) {}
