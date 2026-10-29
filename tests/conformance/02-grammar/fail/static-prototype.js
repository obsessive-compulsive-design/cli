// expect: E000 syntax @4:10
// A static member named `prototype` is removed: prototype manipulation.
class A {
  static prototype = 1
}
