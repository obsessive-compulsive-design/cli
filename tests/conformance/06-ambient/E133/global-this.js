// expect: E133 removed-global-or-member @3:22
// `globalThis` is removed, whatever is read from it.
export const value = globalThis.value
