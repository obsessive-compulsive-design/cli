// expect: E000 syntax @5:12
// `super` is never nullish, so `super?.x` is removed.
class A extends B {
  m() {
    return super?.x
  }
}
