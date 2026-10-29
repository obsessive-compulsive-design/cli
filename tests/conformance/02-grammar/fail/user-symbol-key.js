// expect: E000 syntax @4:3
// A user symbol as a member key is removed: `#private`, well-known symbols only.
class A {
  [Symbol("k")]() {}
}
