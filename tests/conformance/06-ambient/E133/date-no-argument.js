// expect: E133 removed-global-or-member @3:20
// `new Date()` reads the clock unseen: `Date.now()` says so.
export const now = new Date()
