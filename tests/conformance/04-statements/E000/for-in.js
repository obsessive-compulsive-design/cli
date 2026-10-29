// expect: E000 syntax @3:14
// `for…in` is removed: `for…of` over `Object.keys` or `Object.entries`.
for (const k in o) {}
