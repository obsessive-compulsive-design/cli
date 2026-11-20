// expect: E133 removed-global-or-member @3:21
// A symbol of the program's own is removed; the protocol keys are kept.
export const mine = Symbol()

export const key = Symbol.iterator
