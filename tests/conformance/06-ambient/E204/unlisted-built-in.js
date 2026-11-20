// expect: E204 unknown-global @3:21
// `Boolean` is in no row of SPEC.md §6: the set of globals is closed.
export const flag = Boolean(1)
