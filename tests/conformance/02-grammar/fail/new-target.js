// expect: E000 syntax @5:15
// NewTarget is removed: prototype manipulation; a class.
class A {
  constructor() {
    const t = new.target
  }
}
