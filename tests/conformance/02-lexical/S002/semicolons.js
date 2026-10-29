// expect: S002 semicolon @4:12 ";" -> ""
// expect: S002 semicolon @5:13 ";" -> ""
// Every semicolon is style: the canonical layout has none (SPEC.md §9.2).
const a = 1;
export { a };
