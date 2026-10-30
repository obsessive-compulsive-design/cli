// expect: E000 syntax @4:7
// An accessor on an object literal is removed: a class.
const o = {
  get a() {
    return 1
  },
}
