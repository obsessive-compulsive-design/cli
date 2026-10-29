// expect: E201 foreign-import @3:24
// `import(…)` follows the static import rules: a relative `.js` path.
const m = await import("lodash")
