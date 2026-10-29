// expect: E000 syntax @3:26 "==" -> "==="
// `==` coerces its operands: there is one equality, `===`.
const same = (a, b) => a == b
