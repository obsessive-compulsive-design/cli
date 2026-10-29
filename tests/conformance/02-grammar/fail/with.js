// expect: E000 syntax @3:1
// WithStatement is removed: there is no dynamic scope.
with (o) {
  f()
}
