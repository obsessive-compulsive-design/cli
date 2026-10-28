// Every permitted class form: no diagnostic.
class A extends ns.B {
  static count = 0
  static #registry
  x
  #y = 1
  constructor(x) {
    super()
    this.x = x
  }
  get y() {
    return this.#y
  }
  set y(v) {
    this.#y = v
  }
  m(a) {
    return super.m(a)
  }
  async n() {
    await this.m(1)
  }
  *g() {
    yield 1
  }
  static async *h() {}
  static s() {
    return super.s
  }
  [Symbol.iterator]() {
    return this.g()
  }
  static {
    A.count = 1
  }
  #p() {}
  ;
}
