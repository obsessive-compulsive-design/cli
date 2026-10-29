// expect: E000 syntax @4:11 "new A" -> "new A()"
// `new` always takes an argument list.
class A {}
const a = new A
