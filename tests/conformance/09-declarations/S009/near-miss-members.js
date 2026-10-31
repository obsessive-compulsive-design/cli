// Every group in order, a getter with its setter, a static block among the static methods
// and a class of one member: no diagnostic.
class A {
  static count = 0
  static #registry
  static {
    A.count = 1
  }
  static s() {}
  static get b() {}
  x
  #y = 1
  constructor(x) {
    this.x = x
  }
  get y() {
    return this.#y
  }
  set y(v) {
    this.#y = v
  }
  m() {}
  [Symbol.iterator]() {}
  #p() {}
}
class B {
  m() {}
}
