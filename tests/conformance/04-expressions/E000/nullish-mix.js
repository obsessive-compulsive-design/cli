// expect: E000 syntax @3:29
// `??` beside `||` needs parentheses.
const pick = (a, b, c) => a ?? b || c
