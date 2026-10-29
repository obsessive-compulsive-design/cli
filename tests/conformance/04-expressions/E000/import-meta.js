// expect: E000 syntax @3:13
// `import.meta` is removed: a module has no identity at runtime (SPEC.md §5.1).
const url = import.meta.url
