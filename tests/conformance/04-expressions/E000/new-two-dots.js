// expect: E000 syntax @3:21
// `new` names a class as `A` or `ns.A`: one `.` at most.
const a = new ns.sub.A()
