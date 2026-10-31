// expect: S006 line-too-long @2:1
// expect: S009 member-order @4:9 "{\n  x = 1\n  static m() {}\n  static s = 1\n}" -> "{\n  static s = 1\n  static m() {}\n  x = 1\n}"
// Static members come first, fields before methods; the expectation above is over 100 columns.
class A {
  x = 1
  static m() {}
  static s = 1
}
