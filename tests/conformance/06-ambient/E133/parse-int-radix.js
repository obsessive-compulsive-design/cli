// expect: E133 removed-global-or-member @3:22
// Without a radix `Number.parseInt` guesses one from the text.
export const count = Number.parseInt("08")
