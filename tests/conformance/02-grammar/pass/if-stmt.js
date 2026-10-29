// IfStmt: with and without `else`, and an `else if` chain, bodies always blocks.
const x = 1
if (x === 1) {
  f(x)
}
if (x === 2) {
  f(x)
} else {
  f(0)
}
if (x === 3) {
  f(3)
} else if (x === 4) {
  f(4)
} else if (x === 5) {
  f(5)
} else {
  f(6)
}
