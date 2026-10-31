// ClassMember: a field, a method, an accessor and a static block.
class A {
  static {
    A.count = 0
  }
  x = 1
  get y() {
    return this.x
  }
  m() {
    return this.x
  }
}
