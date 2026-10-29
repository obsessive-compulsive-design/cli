// expect: E000 syntax @4:10
try {
  f()
} catch ({ message }) {
  g(message)
}
