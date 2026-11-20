// expect: E133 removed-global-or-member @3:32
// Only `hardwareConcurrency` is kept of `navigator`.
export const agent = navigator.userAgent

export const cores = navigator.hardwareConcurrency
