// AccessorDecl: getters and setters, static or not, on every kind of key.
class A {
  static get b() {
    return 2
  }
  static set b(_v) {}
  get a() {
    return 1
  }
  set a(_v) {}
  get #c() {
    return 3
  }
  set #c(_v) {}
  get [Symbol.iterator]() {
    return this
  }
}

export { A }
