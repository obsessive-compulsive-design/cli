// MethodDecl: static, async, generator and async generator, on every kind of key.
class A {
  static s() {}
  static async *t() {}
  m() {}
  async n() {}
  *g() {}
  async *h() {}
  [Symbol.iterator]() {}
  #p() {}
}
