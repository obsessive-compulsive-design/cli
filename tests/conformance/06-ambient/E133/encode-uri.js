// expect: E133 removed-global-or-member @3:22
// `encodeURI` is removed; `encodeURIComponent` is kept.
export const whole = encodeURI("a b")

export const part = encodeURIComponent("a b")
