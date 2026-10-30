// expect: S002 semicolon @5:12 ";" -> ""
// expect: E001 ambiguous-line-start @6:1
// A `;` does not exempt the line: semicolons are style (S002), and without this
// one the lines would join.
const a = b;
(c + d).e
