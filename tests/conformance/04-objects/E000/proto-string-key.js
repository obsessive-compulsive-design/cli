// expect: E000 syntax @3:13
// A string key `"__proto__"` sets the prototype too, so it is removed as well.
const o = { "__proto__": p }
