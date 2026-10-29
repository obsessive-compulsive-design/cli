// expect: E000 syntax @4:7
// Accessors on object literals are removed: write a class (SPEC.md §12).
const o = {
  get a() {
    return 1
  }
}
