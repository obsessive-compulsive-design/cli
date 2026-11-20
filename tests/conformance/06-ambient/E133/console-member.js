// expect: E133 removed-global-or-member @3:30
// `trace` is not among the members of `console` SPEC.md §6.2 lists.
export const trace = console.trace
