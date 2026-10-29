// expect: E000 syntax @3:15
// `new` names a class: `new A()` or `new ns.A()`.
const a = new (f())()
