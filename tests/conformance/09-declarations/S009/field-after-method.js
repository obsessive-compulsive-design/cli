// expect: S009 member-order @3:9 "{\n  m() {}\n  a = 1\n}" -> "{\n  a = 1\n  m() {}\n}"
// A field after a method: fields come before methods.
class A {
  m() {}
  a = 1
}
