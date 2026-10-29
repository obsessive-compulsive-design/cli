// expect: E000 syntax @5:15
// `new.target` is removed: there is no prototype manipulation (SPEC.md §12).
class A {
  constructor() {
    const t = new.target
  }
}
