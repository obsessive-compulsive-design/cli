// expect: E000 syntax @3:15
// `-2 ** 2` is ambiguous: parenthesize the base or the power.
const power = -2 ** 2
