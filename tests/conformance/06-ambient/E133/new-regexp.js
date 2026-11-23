// expect: E133 removed-global-or-member @3:23
// A pattern built from text is checked by nothing: write a regex literal.
export const digits = new RegExp("\\d+")
