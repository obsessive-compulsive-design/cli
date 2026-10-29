// MethodDecl: static, async, generator and async generator, on every kind of key.
class A {
  m() {}
  async n() {}
  *g() {}
  async *h() {}
  static s() {}
  static async *t() {}
  #p() {}
  [Symbol.iterator]() {}
}
