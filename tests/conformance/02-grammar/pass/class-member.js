// ClassMember: a field, a method, an accessor and a static block.
class A {
  x = 1
  m() {
    return this.x
  }
  get y() {
    return this.x
  }
  static {
    A.count = 0
  }
}
