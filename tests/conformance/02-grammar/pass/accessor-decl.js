// AccessorDecl: getters and setters, static or not, on every kind of key.
class A {
  get a() {
    return 1
  }
  set a(v) {}
  static get b() {
    return 2
  }
  static set b(v) {}
  get #c() {
    return 3
  }
  set #c(v) {}
  get [Symbol.iterator]() {
    return this
  }
}
