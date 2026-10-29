// expect: E001 ambiguous-line-start @5:3
// A name on the line after `break` reads as its label but is a separate statement.
while (x) {
  break
  outer
}
