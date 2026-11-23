// MethodDecl: static, async, generator and async generator, on every kind of key.
class A {
  static s() {}
  static async *t() {
    yield await Promise.resolve(1)
  }
  m() {}
  async n() {
    await Promise.resolve(1)
  }
  *g() {}
  async *h() {
    yield await Promise.resolve(1)
  }
  [Symbol.iterator]() {}
  #p() {}
}

export { A }
