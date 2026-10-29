// expect: E000 syntax @5:12
// OptionalChain on `super` is removed: `super` is never nullish.
class A extends B {
  m() {
    return super?.x
  }
}
